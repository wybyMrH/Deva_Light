use crate::aggregator::StateAggregator;
use crate::config::{
    ensure_http_token, load_runtime_config, save_app_config, save_runtime_config, AppConfig,
    RuntimeConfig,
};
use crate::logging::{log_error, log_info, log_warn};
use crate::types::{Status, Tool};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookEvent {
    pub event_type: String,
    #[serde(default = "default_session_id", alias = "sessionId")]
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default, alias = "tool", alias = "toolName", alias = "tool_name")]
    pub tool_call: Option<String>,
    #[serde(
        default,
        alias = "prompt",
        alias = "user_prompt",
        alias = "message",
        alias = "task"
    )]
    pub task_hint: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

impl HookEvent {
    pub fn resolve_status(&self) -> Option<Status> {
        let status = Self::event_type_to_status(&self.event_type)?;

        // Cursor shows approval UI after preToolUse; beforeShellExecution only fires
        // after the user accepts, so we mark Waiting as soon as a tool is proposed.
        if self.source.as_deref() == Some("cursor") && self.event_type == "pre-tool-use" {
            return Some(Status::Waiting);
        }

        Some(status)
    }

    pub fn event_type_to_status(event_type: &str) -> Option<Status> {
        match event_type {
            "session-start" => Some(Status::Idle),
            "prompt-submit" | "before-submit-prompt" => Some(Status::Working),
            "pre-tool-use" => Some(Status::Working),
            "permission-request" => Some(Status::Waiting),
            "post-tool-use" | "post-tool-use-failure" => Some(Status::Working),
            "notification" => Some(Status::Waiting),
            // Cursor approval gates (no Claude PermissionRequest equivalent)
            "before-shell-execution" | "before-mcp-execution" | "before-read-file" => {
                Some(Status::Waiting)
            }
            "after-shell-execution"
            | "after-mcp-execution"
            | "after-file-edit"
            | "after-agent-response"
            | "after-agent-thought" => Some(Status::Working),
            "subagent-start" | "subagent-stop" | "pre-compact" => Some(Status::Working),
            "stop" => Some(Status::Done),
            "session-end" => None,
            _ => None,
        }
    }

    pub fn resolve_tool(&self) -> Tool {
        if self.source.as_deref() == Some("cursor") {
            return Tool::Cursor;
        }
        Tool::ClaudeCode
    }

    pub fn is_subagent_event(&self) -> bool {
        matches!(self.event_type.as_str(), "subagent-start" | "subagent-stop")
    }

    pub fn should_track(&self) -> bool {
        if self.session_id != "unknown" {
            return true;
        }

        matches!(self.event_type.as_str(), "session-start" | "session-end")
    }
}

pub struct HttpServerController {
    shutdown: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    port: Mutex<Option<u16>>,
}

impl HttpServerController {
    pub fn new() -> Self {
        Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            port: Mutex::new(None),
        }
    }

    pub fn current_port(&self) -> Option<u16> {
        *self.port.lock()
    }

    pub fn start(
        &self,
        aggregator: Arc<StateAggregator>,
        app_config: &AppConfig,
    ) -> Result<u16, String> {
        self.stop();

        let shutdown = Arc::clone(&self.shutdown);
        let (port, worker) = spawn_http_server(Arc::clone(&aggregator), app_config, shutdown)
            .map_err(|error| error.to_string())?;

        *self.worker.lock() = Some(worker);
        *self.port.lock() = Some(port);
        Ok(port)
    }

    pub fn restart(
        &self,
        aggregator: Arc<StateAggregator>,
        app_config: &AppConfig,
    ) -> Result<u16, String> {
        self.start(aggregator, app_config)
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);

        if let Some(worker) = self.worker.lock().take() {
            let _ = worker.join();
        }

        *self.port.lock() = None;
        self.shutdown.store(false, Ordering::SeqCst);
    }
}

impl Default for HttpServerController {
    fn default() -> Self {
        Self::new()
    }
}

pub fn parse_hook_event(payload: &str) -> Result<HookEvent, serde_json::Error> {
    serde_json::from_str(payload)
}

pub fn existing_instance_is_healthy() -> bool {
    let Some(runtime) = load_runtime_config() else {
        return false;
    };

    let address = SocketAddr::from(([127, 0, 0, 1], runtime.http_port));

    TcpStream::connect_timeout(&address, Duration::from_millis(250))
        .and_then(|mut stream| {
            stream.set_read_timeout(Some(Duration::from_millis(250)))?;
            stream.set_write_timeout(Some(Duration::from_millis(250)))?;
            stream.write_all(
                b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )?;

            let mut response = String::new();
            stream.read_to_string(&mut response)?;
            Ok(response.starts_with("HTTP/1.1 200 OK"))
        })
        .unwrap_or(false)
}

pub fn start_http_server(
    aggregator: Arc<StateAggregator>,
    app_config: &AppConfig,
) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let shutdown = Arc::new(AtomicBool::new(false));
    let (port, worker) = spawn_http_server(aggregator, app_config, shutdown)?;
    drop(worker);
    Ok(port)
}

fn spawn_http_server(
    aggregator: Arc<StateAggregator>,
    app_config: &AppConfig,
    shutdown: Arc<AtomicBool>,
) -> Result<(u16, JoinHandle<()>), Box<dyn std::error::Error + Send + Sync>> {
    let mut persisted_config = app_config.clone();
    let auth_token = ensure_http_token(&mut persisted_config);
    if persisted_config.http_token != app_config.http_token {
        save_app_config(&persisted_config)?;
    }

    let bind_address = format!(
        "{}:{}",
        app_config.http_bind,
        app_config.http_port.unwrap_or(0)
    );
    let listener = TcpListener::bind(bind_address)?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();

    save_runtime_config(&RuntimeConfig {
        http_port: port,
        http_token: auth_token.clone(),
    })?;
    log_info(
        "http_server",
        format!(
            "listening on {}:{}",
            app_config.http_bind,
            app_config.http_port.unwrap_or(port)
        ),
    );

    let worker = thread::Builder::new()
        .name("ai-light-http-server".to_string())
        .spawn(move || run_http_server(listener, aggregator, auth_token, shutdown))?;

    Ok((port, worker))
}

fn run_http_server(
    listener: TcpListener,
    aggregator: Arc<StateAggregator>,
    auth_token: Option<String>,
    shutdown: Arc<AtomicBool>,
) {
    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let aggregator = Arc::clone(&aggregator);
                let auth_token = auth_token.clone();
                thread::spawn(move || {
                    if let Err(error) = handle_connection(stream, aggregator, auth_token) {
                        log_error(
                            "http_server",
                            format!("connection handling failed: {error}"),
                        );
                    }
                });
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => {
                if shutdown.load(Ordering::Relaxed) {
                    break;
                }
                log_warn(
                    "http_server",
                    format!("failed to accept incoming connection: {error}"),
                );
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    log_info("http_server", "stopped");
}

fn default_session_id() -> String {
    "unknown".to_string()
}

fn handle_connection(
    mut stream: TcpStream,
    aggregator: Arc<StateAggregator>,
    auth_token: Option<String>,
) -> io::Result<()> {
    let request = read_http_request(&mut stream)?;
    let Some((request_line, headers, body)) = request else {
        log_warn("http_server", "received malformed HTTP request");
        return write_response(&mut stream, 400, "Bad Request", "missing request");
    };

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();

    match (method, path) {
        ("GET", "/health") => write_response(&mut stream, 200, "OK", "ok"),
        ("GET", "/state") => {
            let body = serde_json::to_string(&aggregator.get_lights())
                .unwrap_or_else(|_| "[]".to_string());
            write_json_response(&mut stream, 200, "OK", &body)
        }
        ("POST", "/events") => {
            if !authorize_request(&headers, auth_token.as_deref()) {
                log_warn("http_server", "rejected unauthorized hook event");
                return write_response(&mut stream, 401, "Unauthorized", "invalid token");
            }

            match parse_hook_event(&body) {
                Ok(event) => {
                    apply_hook_event(&aggregator, event);
                    write_response(&mut stream, 200, "OK", "ok")
                }
                Err(error) => {
                    log_warn("http_server", format!("invalid hook payload: {error}"));
                    write_response(&mut stream, 400, "Bad Request", "invalid json")
                }
            }
        }
        _ => {
            log_warn("http_server", format!("unhandled request {method} {path}"));
            write_response(&mut stream, 404, "Not Found", "not found")
        }
    }
}

fn authorize_request(headers: &str, expected_token: Option<&str>) -> bool {
    let Some(expected_token) = expected_token else {
        return true;
    };

    extract_header_value(headers, "x-deva-light-token")
        .or_else(|| extract_bearer_token(headers))
        .is_some_and(|provided| provided == expected_token)
}

fn extract_header_value(headers: &str, header_name: &str) -> Option<String> {
    let target = header_name.to_ascii_lowercase();

    headers.lines().skip(1).find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case(&target) {
            Some(value.trim().to_string())
        } else {
            None
        }
    })
}

fn extract_bearer_token(headers: &str) -> Option<String> {
    let authorization = extract_header_value(headers, "authorization")?;
    authorization
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToString::to_string)
}

fn read_http_request(stream: &mut TcpStream) -> io::Result<Option<(String, String, String)>> {
    let mut buffer = Vec::new();
    let mut chunk = [0; 1024];

    loop {
        let bytes_read = stream.read(&mut chunk)?;
        if bytes_read == 0 {
            break;
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);

        if let Some((header_end, separator_len)) = find_header_split(&buffer) {
            let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
            let request_line = headers.lines().next().unwrap_or_default().to_string();
            let content_length = parse_content_length(&headers);
            let body_start = header_end + separator_len;

            while buffer.len() < body_start + content_length {
                let bytes_read = stream.read(&mut chunk)?;
                if bytes_read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..bytes_read]);
            }

            let body_end = (body_start + content_length).min(buffer.len());
            let body = String::from_utf8_lossy(&buffer[body_start..body_end]).to_string();

            return Ok(Some((request_line, headers, body)));
        }

        if buffer.len() > 64 * 1024 {
            return Ok(None);
        }
    }

    Ok(None)
}

fn apply_hook_event(aggregator: &StateAggregator, event: HookEvent) {
    if !event.should_track() {
        log_info(
            "http_server",
            format!(
                "ignored {} without conversation/session id",
                event.event_type
            ),
        );
        return;
    }

    let tool = event.resolve_tool();
    log_info(
        "http_server",
        format!(
            "event {} session={} source={:?} cwd={} tool={}",
            event.event_type,
            event.session_id,
            event.source,
            event.cwd.as_deref().unwrap_or("-"),
            event.tool_call.as_deref().unwrap_or("-")
        ),
    );

    match event.event_type.as_str() {
        "session-start" => {
            let cwd = resolve_event_cwd(&event);
            aggregator.add_session(event.session_id, tool, &cwd, Status::Idle);
        }
        "session-end" => {
            aggregator.remove_session(&event.session_id);
        }
        _ => {
            ensure_session_exists(aggregator, &event, tool);

            if should_ignore_late_event_after_done(aggregator, &event) {
                log_info(
                    "http_server",
                    format!(
                        "ignored late {} for completed session {}",
                        event.event_type, event.session_id
                    ),
                );
                return;
            }

            if let Some(status) = event.resolve_status() {
                if should_apply_status_transition(
                    aggregator.session_status(&event.session_id),
                    status,
                    &event.event_type,
                    event.source.as_deref(),
                ) {
                    aggregator.update_session_status(&event.session_id, status);
                }
            }

            if let Some(task_hint) = event.task_hint.as_deref().filter(|value| !value.is_empty()) {
                aggregator.set_task_name(&event.session_id, task_hint.to_string());
            }

            if let Some(tool_call) = event.tool_call {
                aggregator.set_last_tool_call(&event.session_id, tool_call);
            }
        }
    }
}

fn resolve_event_cwd(event: &HookEvent) -> PathBuf {
    event
        .cwd
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn ensure_session_exists(aggregator: &StateAggregator, event: &HookEvent, tool: Tool) {
    if aggregator.session_status(&event.session_id).is_some() {
        return;
    }

    if event.is_subagent_event() {
        log_info(
            "http_server",
            format!(
                "skipped orphan subagent {} for parent session {}",
                event.event_type, event.session_id
            ),
        );
        return;
    }

    let cwd = resolve_event_cwd(event);
    aggregator.add_session(event.session_id.clone(), tool, &cwd, Status::Idle);
    log_info(
        "http_server",
        format!(
            "auto-created session {} from orphan event {}",
            event.session_id, event.event_type
        ),
    );
}

fn should_apply_status_transition(
    current: Option<Status>,
    next: Status,
    event_type: &str,
    source: Option<&str>,
) -> bool {
    let Some(current) = current else {
        return true;
    };

    if current != Status::Waiting || next != Status::Working {
        return true;
    }

    if source == Some("cursor") {
        return matches!(
            event_type,
            "post-tool-use"
                | "post-tool-use-failure"
                | "after-shell-execution"
                | "after-mcp-execution"
                | "after-file-edit"
                | "stop"
                | "session-end"
        );
    }

    matches!(
        event_type,
        "post-tool-use" | "after-shell-execution" | "after-mcp-execution" | "stop"
    )
}

fn should_ignore_late_event_after_done(aggregator: &StateAggregator, event: &HookEvent) -> bool {
    if aggregator.session_status(&event.session_id) != Some(Status::Done) {
        return false;
    }

    matches!(
        event.event_type.as_str(),
        "pre-tool-use"
            | "permission-request"
            | "post-tool-use"
            | "notification"
            | "before-shell-execution"
            | "before-mcp-execution"
            | "before-read-file"
    )
}

fn find_header_split(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            buffer
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
}

fn parse_content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn write_response(stream: &mut TcpStream, code: u16, reason: &str, body: &str) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn write_json_response(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    body: &str,
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {code} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_token_when_auth_enabled() {
        assert!(!authorize_request("", Some("secret")));
    }

    #[test]
    fn accepts_matching_header_token() {
        let headers = "POST /events HTTP/1.1\r\nX-Deva-Light-Token: secret\r\n";
        assert!(authorize_request(headers, Some("secret")));
    }

    #[test]
    fn accepts_bearer_token() {
        let headers = "POST /events HTTP/1.1\r\nAuthorization: Bearer secret\r\n";
        assert!(authorize_request(headers, Some("secret")));
    }

    #[test]
    fn cursor_waiting_persists_across_agent_response() {
        assert!(!should_apply_status_transition(
            Some(Status::Waiting),
            Status::Working,
            "after-agent-response",
            Some("cursor"),
        ));
        assert!(should_apply_status_transition(
            Some(Status::Waiting),
            Status::Working,
            "after-shell-execution",
            Some("cursor"),
        ));
        assert!(should_apply_status_transition(
            Some(Status::Waiting),
            Status::Working,
            "post-tool-use",
            Some("cursor"),
        ));
    }

    #[test]
    fn cursor_pre_tool_use_maps_to_waiting() {
        let event = HookEvent {
            event_type: "pre-tool-use".to_string(),
            session_id: "conv-1".to_string(),
            cwd: None,
            tool_call: Some("Shell".to_string()),
            task_hint: None,
            source: Some("cursor".to_string()),
        };

        assert_eq!(event.resolve_status(), Some(Status::Waiting));
    }
}
