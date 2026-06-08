# Ubuntu Hook-Only Forwarding

Last updated: 2026-05-31

Use this mode when:

- Deva Light GUI runs on Windows.
- Claude Code runs on Ubuntu over SSH.
- Ubuntu should not install or show the Deva Light desktop widget.
- Ubuntu only forwards Claude Code hook events to the Windows host.

## Architecture

```text
Claude Code on Ubuntu
-> ~/.deva_light/bin/deva-light-hook
-> AI_LIGHT_URL=http://WINDOWS_IP:17321/events
-> Deva Light on Windows
-> Windows desktop light changes
```

## Windows Host Setup

Deva Light on Windows must listen on a fixed LAN port.

Edit:

```text
%USERPROFILE%\.deva_light\config.json
```

Recommended config:

```json
{
  "http_bind": "0.0.0.0",
  "http_port": 17321
}
```

Restart Deva Light after editing the config.

Windows Firewall must allow inbound TCP traffic on the selected port, for example `17321`.

## Ubuntu Client Setup

From a checkout of this repository on Ubuntu:

```bash
./scripts/install-ubuntu-hook.sh http://WINDOWS_IP:17321
```

Example:

```bash
./scripts/install-ubuntu-hook.sh http://192.0.2.10:17321
```

The script installs:

```text
~/.deva_light/bin/deva-light-hook
```

and merges Claude Code hooks into:

```text
~/.claude/settings.json
```

It backs up the previous settings file before writing.

## Existing Hook Binary

If you already have a Linux `deva-light-hook` binary:

```bash
AI_LIGHT_HOOK_SOURCE=/path/to/deva-light-hook \
  ./scripts/install-ubuntu-hook.sh http://WINDOWS_IP:17321
```

If no binary is provided, the script tries to build it with Cargo:

```bash
cargo build -p deva-light-hook --release
```

## Verify

On Windows:

```powershell
$runtime = Get-Content "$env:USERPROFILE\.deva_light\runtime.json" | ConvertFrom-Json
Invoke-WebRequest -UseBasicParsing "http://127.0.0.1:$($runtime.http_port)/health" |
  Select-Object -ExpandProperty Content
```

Expected:

```text
ok
```

On Ubuntu:

```bash
AI_LIGHT_URL=http://WINDOWS_IP:17321 \
  ~/.deva_light/bin/deva-light-hook session-start <<'JSON'
{"session_id":"ubuntu-test","cwd":"/tmp/ubuntu-test"}
JSON
```

The Windows Deva Light widget should show a project light for `/tmp/ubuntu-test`.

## Notes

- Ubuntu does not run the Tauri GUI in this mode.
- Ubuntu does not need `runtime.json`.
- `AI_LIGHT_URL` may include `/events`, but the hook also works if it is omitted.
- Use LAN mode only on trusted networks. For untrusted networks, prefer SSH tunneling.
