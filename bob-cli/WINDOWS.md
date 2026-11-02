# orbix CLI on Windows

The `orbix` CLI builds and runs on Windows with **only the Rust toolchain** -
no Visual Studio C++ workload. Unlike the desktop app (which needs `whisper-rs`
and the `windows` crate), the CLI is pure Rust with `rustls` TLS, so a plain
`cargo build` produces `orbix.exe`.

Its main job on Windows: **chat with an Orbix host running on another device**
(a Mac, or another Windows PC) over your local network.

## Build

One-time: install Rust from <https://rustup.rs> (accept the defaults). Then in
a fresh PowerShell, from the `bob-cli` directory:

```pwsh
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

That builds `orbix.exe` + `bob.exe` in release mode, drops them in
`%LOCALAPPDATA%\Orbix\bin`, and adds that folder to your user PATH. Open a new
terminal afterwards and `orbix` works from anywhere. (Override the location
with `$env:ORBIX_INSTALL_DIR` before running.)

Prefer to do it by hand? Just build and copy the binary yourself:

```pwsh
cargo build --release          # -> .\target\release\orbix.exe (and bob.exe)
```

## Connect to a host and chat

On the **host** device (the Mac or PC actually running Orbix Core): open
**Settings → Models → Expose local model endpoint** and turn it on. Copy the
token shown there. Both devices must be on the same Wi-Fi / LAN.

Then on this Windows PC:

```pwsh
# Discover hosts on the network, pick one, chat. No IP to type.
orbix connect

#   Found 1 host:
#     1) Anna's MacBook  (macos · gpt-oss-20b)  http://192.168.1.20:51231
#   Pick a host [1-1]: 1
#   Token for Anna's MacBook (from that device's Settings > Models): orbx-...
#   → hello Bob
```

Add `--save` to remember the host + token so next time it's just `orbix chat`.

If discovery can't find anything (some networks block mDNS), point at the host
directly - the endpoint is standard OpenAI-compatible:

```pwsh
orbix chat --endpoint http://192.168.1.20:51231 --token orbx-... --save
```

Inside the chat REPL: `/connect` switches to a different host, `:q` or Ctrl-D
exits.

## No `orbix` yet? Any OpenAI client works

Because the host speaks OpenAI-compatible `/v1`, you don't strictly need this
CLI. `curl` (bundled with Windows 10/11) works too:

```pwsh
curl http://192.168.1.20:51231/v1/chat/completions `
  -H "Authorization: Bearer orbx-..." `
  -H "Content-Type: application/json" `
  -d "{\"messages\":[{\"role\":\"user\",\"content\":\"hello\"}]}"
```

## Notes

- **Same network required.** The address is a LAN IP; it won't reach across the
  internet without a VPN/tunnel. Don't expose port 51231 raw to the internet.
- **The host must be awake** with the app open and a model loaded.
- **Windows Firewall** may prompt the first time the host advertises; allow it
  on private networks.
- Hosting *from* Windows works too: open the desktop app on this PC, turn on
  **Settings → Models → Expose local model endpoint**, and a Mac (or any
  OpenAI client) connects to the LAN URL it shows. The CLI half (this) is
  client-and-host-agnostic.
