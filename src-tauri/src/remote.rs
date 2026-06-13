use crate::config::{ensure_http_token, load_app_config, save_app_config, AppConfig};
use crate::ssh_remote::discover_codex_sessions_dir;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::process::Command;

struct AddressCache {
    addresses: Vec<String>,
    fetched_at: Instant,
}

static ADDRESS_CACHE: OnceLock<Mutex<Option<AddressCache>>> = OnceLock::new();

fn address_cache() -> &'static Mutex<Option<AddressCache>> {
    ADDRESS_CACHE.get_or_init(|| Mutex::new(None))
}

const ADDRESS_CACHE_TTL: Duration = Duration::from_secs(120);

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
    pub ssh_targets: Vec<SshTargetSetupInfo>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTargetSetupInfo {
    pub target: String,
    pub identity_file: Option<String>,
    pub ssh_install_command: Option<String>,
    pub ssh_codex_path: Option<String>,
}

pub fn build_remote_setup_info(probe_ssh: bool) -> Result<RemoteSetupInfo, String> {
    let mut config = load_app_config();
    let token = ensure_http_token(&mut config);
    if token.is_some() && config.http_token.is_some() {
        save_app_config(&config).map_err(|error| error.to_string())?;
    }

    let runtime_port = crate::config::load_runtime_config().map(|runtime| runtime.http_port);
    let port = config.http_port.or(runtime_port).unwrap_or(17_321);
    // Only enumerate LAN adapters when LAN forwarding is enabled; localhost mode
    // never needs it and avoids spawning PowerShell entirely.
    let addresses = if config.http_bind == "0.0.0.0" {
        detect_local_addresses()
    } else {
        vec![Ipv4Addr::LOCALHOST.to_string()]
    };
    let host = select_primary_host(&config, &addresses).unwrap_or_else(|| {
        addresses
            .first()
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string())
    });
    let event_url = format!("http://{host}:{port}/events");

    let install_command = build_install_command(&event_url, token.as_deref());
    let curl_install_command = build_curl_install_command(&event_url, token.as_deref());
    let ssh_targets: Vec<SshTargetSetupInfo> = config
        .normalized_ssh_targets()
        .into_iter()
        .map(|entry| {
            let ssh_install_command = Some(build_ssh_install_command(
                &entry.target,
                &curl_install_command,
            ));
            let ssh_codex_path = if probe_ssh {
                config
                    .remote_codex_via_ssh
                    .then(|| discover_codex_sessions_dir(&entry.target))
                    .flatten()
                    .map(|path| path.to_string_lossy().to_string())
            } else {
                None
            };

            SshTargetSetupInfo {
                target: entry.target,
                identity_file: entry.identity_file,
                ssh_install_command,
                ssh_codex_path,
            }
        })
        .collect();
    let ssh_install_command = ssh_targets
        .first()
        .and_then(|entry| entry.ssh_install_command.clone());
    let ssh_codex_path = ssh_targets
        .first()
        .and_then(|entry| entry.ssh_codex_path.clone());

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
        ssh_targets,
    })
}

pub fn detect_local_addresses() -> Vec<String> {
    detect_local_addresses_inner(false)
}

pub fn detect_local_addresses_fresh() -> Vec<String> {
    detect_local_addresses_inner(true)
}

fn detect_local_addresses_inner(force_refresh: bool) -> Vec<String> {
    if !force_refresh {
        if let Ok(cache) = address_cache().lock() {
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < ADDRESS_CACHE_TTL {
                    return entry.addresses.clone();
                }
            }
        }
    }

    let addresses = collect_local_addresses();

    if let Ok(mut cache) = address_cache().lock() {
        *cache = Some(AddressCache {
            addresses: addresses.clone(),
            fetched_at: Instant::now(),
        });
    }

    addresses
}

fn collect_local_addresses() -> Vec<String> {
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
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        if let Ok(output) = Command::new("powershell")
            .creation_flags(CREATE_NO_WINDOW)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
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
