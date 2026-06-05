use crate::config::{ensure_http_token, load_app_config, save_app_config, AppConfig};
use crate::ssh_remote::discover_codex_sessions_dir;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
#[cfg(windows)]
use std::process::Command;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSetupInfo {
    pub http_bind: String,
    pub http_port: Option<u16>,
    pub runtime_port: Option<u16>,
    pub http_token: Option<String>,
    pub local_addresses: Vec<String>,
    pub primary_host: Option<String>,
    pub install_command: String,
    pub curl_install_command: String,
    pub ssh_install_command: Option<String>,
    pub ssh_codex_path: Option<String>,
}

pub fn build_remote_setup_info() -> Result<RemoteSetupInfo, String> {
    let mut config = load_app_config();
    let token = ensure_http_token(&mut config);
    if token.is_some() && config.http_token.is_some() {
        save_app_config(&config).map_err(|error| error.to_string())?;
    }

    let runtime_port = crate::config::load_runtime_config().map(|runtime| runtime.http_port);
    let port = config.http_port.or(runtime_port).unwrap_or(17_321);
    let addresses = detect_local_addresses();
    let host = select_primary_host(&config, &addresses)
        .unwrap_or_else(|| addresses.first().cloned().unwrap_or_else(|| "127.0.0.1".to_string()));
    let event_url = format!("http://{host}:{port}/events");

    let install_command = build_install_command(&event_url, token.as_deref());
    let curl_install_command = build_curl_install_command(&event_url, token.as_deref());
    let ssh_install_command = config
        .remote_ssh_target
        .as_deref()
        .map(|target| build_ssh_install_command(target, &curl_install_command));
    let ssh_codex_path = config
        .remote_codex_via_ssh
        .then(|| {
            config
                .remote_ssh_target
                .as_deref()
                .and_then(discover_codex_sessions_dir)
        })
        .flatten()
        .map(|path| path.to_string_lossy().to_string());

    Ok(RemoteSetupInfo {
        http_bind: config.http_bind.clone(),
        http_port: config.http_port,
        runtime_port,
        http_token: token,
        local_addresses: addresses,
        primary_host: Some(host.clone()),
        install_command,
        curl_install_command,
        ssh_install_command,
        ssh_codex_path,
    })
}

pub fn detect_local_addresses() -> Vec<String> {
    let mut addresses = Vec::new();

    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                if let IpAddr::V4(ipv4) = local_addr.ip() {
                    if !ipv4.is_loopback() {
                        addresses.push(ipv4.to_string());
                    }
                }
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(output) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike '127.*' -and $_.PrefixOrigin -ne 'WellKnown' }).IPAddress",
            ])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let ip = line.trim();
                    if !ip.is_empty() && !addresses.iter().any(|existing| existing == ip) {
                        addresses.push(ip.to_string());
                    }
                }
            }
        }
    }

    if addresses.is_empty() {
        addresses.push(Ipv4Addr::LOCALHOST.to_string());
    }

    addresses
}

fn select_primary_host(config: &AppConfig, addresses: &[String]) -> Option<String> {
    if config.http_bind == "127.0.0.1" {
        return addresses.first().cloned();
    }

    addresses
        .iter()
        .find(|address| *address != "127.0.0.1")
        .or_else(|| addresses.first())
        .cloned()
}

fn build_install_command(event_url: &str, token: Option<&str>) -> String {
    let mut parts = vec![format!("AI_LIGHT_URL={}", shell_quote(event_url))];

    if let Some(token) = token {
        parts.push(format!("AI_LIGHT_TOKEN={}", shell_quote(token)));
    }

    parts.push("./install-ubuntu-hook.sh".to_string());
    parts.join(" ")
}

fn build_curl_install_command(event_url: &str, token: Option<&str>) -> String {
    let mut env_prefix = format!("AI_LIGHT_URL={}", shell_quote(event_url));

    if let Some(token) = token {
        env_prefix.push(' ');
        env_prefix.push_str(&format!("AI_LIGHT_TOKEN={}", shell_quote(token)));
    }

    format!(
        "curl -fsSL https://github.com/wybyMrH/Deva_Light/releases/latest/download/install-ubuntu-hook.sh | {env_prefix} bash -s -- {url}",
        url = shell_quote(event_url)
    )
}

fn build_ssh_install_command(target: &str, remote_command: &str) -> String {
    format!(
        "ssh {} 'curl -fsSL https://github.com/wybyMrH/Deva_Light/releases/latest/download/install-ubuntu-hook.sh | {}'",
        shell_quote(target),
        remote_command.replace('\'', "'\"'\"'")
    )
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }

    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | ':' | '_' | '-'))
    {
        return value.to_string();
    }

    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
