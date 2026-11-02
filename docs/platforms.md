# Platforms

Orbix Core is one codebase for every desktop platform. The React front end is shared unchanged, the Rust backend is shared except for a thin platform layer behind `#[cfg(target_os = "...")]`, and Tauri produces the native bundle. This page says where each platform stands, what "supported" means, what is shared and what is not, and how a new platform gets built.

## Status

| Platform | Status | Ships as | Notes |
|---|---|---|---|
| **macOS** 13+ (Apple Silicon) | Reference platform. Every feature lands here first and is used daily. | `.dmg` | Intel builds are produced by the release workflow but are not tested daily. |
| **Windows** 10/11 (x64) | Merged into this repository and building in CI. Ships with the open-source launch. One app bridge so far (classic Outlook over COM); installers are unsigned until an Authenticode certificate is enrolled. | NSIS `.exe` installer, MSI | `llama-server` build picked per machine: CUDA on NVIDIA, Vulkan on other GPUs, CPU otherwise, with automatic CPU fallback. |
| **Linux** (x64) | Next. Not started, and nothing in the shared code forbids it. | AppImage, `.deb` (planned) | The work list is at the bottom of this page. Contributions welcome. |

**Supported** means all of the following: the app builds in CI on that OS, the backend tests pass there, the release workflow produces an artifact with a checksum (signed where the platform supports it), and every platform integration in the table below either works or tells the user plainly that it is not available yet. Until a platform meets all four it is "in progress", whatever the demo looks like.

## What is shared and what is not

Identical on every platform: the front end, Bob and the agent loop, the tool catalog and its policy, workflows and durable runs, agents and shifts, Imagine, connectors and the Deno sandbox, the egress gateway and compliance policy, storage formats, the model catalog, the `orbix` CLI, and the browser extension.

Platform-specific, one Rust module per concern, always returning the same JSON shape to the front end:

| Concern | macOS | Windows | Linux (planned) |
|---|---|---|---|
| Local inference | `llama-server` Metal build | `llama-server` CUDA, Vulkan, or CPU build, picked per machine (`ORBIX_LLAMA_VARIANT=cpu|cuda|vulkan` overrides) | `llama-server` CPU or Vulkan build |
| Speech-to-text | whisper.cpp | whisper.cpp (needs the MSVC C++ toolchain to build) | whisper.cpp |
| Text-to-speech | Kokoro, plus `orbix-speak` (AVSpeechSynthesizer) | Kokoro, plus SAPI through PowerShell `System.Speech` | Kokoro, plus speech-dispatcher or Piper |
| App bridges (Calendar, Mail, Notes, Reminders, Messages, Outlook) | AppleScript bridges in `resources/bridges/` | Outlook only, classic Outlook through its COM object model driven from PowerShell (`windows_script` in the same manifests). The new Outlook has no scripting interface: the bridge reports it as limited and mail and calendar go through the connected account | Not yet. Thunderbird and Evolution over D-Bus are candidates |
| Window reading and field automation | Accessibility API (`window_intelligence::ax`) | UI Automation (`window_intelligence::uia`) | AT-SPI2 |
| Front app, browser tabs and URL, screenshots | CoreGraphics, AppleScript | Win32 `GetForegroundWindow`, UIA (tab strip), `System.Drawing` | X11 / Wayland portals |
| App control, clipboard, system info | AppleScript via `osascript` | PowerShell (`Start-Process`, `Get-CimInstance`, `Get-Clipboard`) | D-Bus, `xdg-open`, `wl-clipboard` |
| Notifications | Notification Center | WinRT toast under the registered AppUserModelID `ai.orbixcore.app` | D-Bus notifications |
| Master key for encrypted secrets | Keychain (`security`) | DPAPI (`CryptProtectData`, per-user), wrapped key file in the data folder | Secret Service (`libsecret`) |
| Data folder | `~/Library/Application Support/ai.orbixcore.app/` | `%LOCALAPPDATA%\ai.orbixcore.app\` | `~/.local/share/ai.orbixcore.app/` |
| Bundle | `.dmg`, signed and notarized when the Apple secrets exist | NSIS and WiX; Authenticode signing through `bundle.windows.signCommand` when the signing variables exist (`scripts/SIGNING.md`) | AppImage and `.deb` |
| CI runner | `macos-14` | `windows-latest` | `ubuntu-latest` |

The modules that carry the platform layer: `voice.rs`, `os_auto.rs`, `window_intelligence.rs`, `screen_capture.rs`, `imagine.rs` (browser tabs), `local_llm.rs` (binary variant, download URL, memory query), `storage.rs` (master key), `tools.rs` (shell), `process_ext.rs` (Windows: children without console windows), `app_bridge.rs`, `oauth.rs` (only a path hint), `build.rs`, the `Cargo.toml` target tables, and the Tauri config: `tauri.conf.json` is the shared base, `tauri.macos.conf.json` and `tauri.windows.conf.json` carry the window chrome, bundle resources, and installer settings for each platform (Tauri merges the platform file over the base at build time). Prompt wording that names the OS lives in compile-time consts in `llm_engine.rs` (`HOST_OS_NAME` and friends), never in a string literal.

## Rules for platform code

1. **Gate, do not fork.** Platform code lives behind `#[cfg(target_os = "...")]` in the same module as its siblings. Dependencies go under `[target.'cfg(target_os = "...")'.dependencies]` in `Cargo.toml`. `build.rs` links Apple frameworks only on macOS. Per-platform Tauri settings go in `tauri.<platform>.conf.json`, never by editing the base for one OS. A separate tree per platform is what the Windows port started as, and merging it back was the cost of that; do not repeat it.
2. **Same shape out.** A platform implementation returns the JSON the front end already consumes. The front end has no platform branches and must not grow any.
3. **Say no honestly.** A capability that does not exist on a platform returns a clear error the UI can show ("App bridges are not available on this platform yet"), never a silent success or a fake result.
4. **Keep the other platforms green.** Do not delete another platform's gated code because it does not compile on yours. Once a platform is in CI, a change from any platform must keep its job passing.
5. **A platform is a CI job and a release artifact, nothing more.** Adding one means adding its runner to `.github/workflows/ci.yml` and its matrix entry to `.github/workflows/release.yml`. No platform-specific repositories, branches, or forks.

## Building on each platform

**macOS.** Xcode command line tools, Rust (stable), Node.js 18+. Then `cd orbixcore && npm install && npm run tauri dev`. Release bundles: `npm run tauri:build` (stages the CLI and the Swift speech helper first).

**Windows.** Rust via rustup, Node.js LTS, Visual Studio Build Tools 2022 with the "Desktop development with C++" workload (whisper.cpp and the `windows` crate need it). Clone with `git config --global core.autocrlf false` (the repository pins LF through `.gitattributes`) and keep the editor on UTF-8: a cp1252 save once turned the box-drawing comment rules into `â”€` mojibake. Then, in PowerShell, `cd orbixcore; npm install; npm run tauri dev`. The first build takes 5 to 15 minutes; after that it is incremental. `npm run tauri:build` stages `orbix.exe`/`bob.exe` through `scripts/build-bob.ps1` and produces the NSIS and MSI installers. The CLI alone builds with only the Rust toolchain: `bob-cli\install.ps1` (see `bob-cli/WINDOWS.md`).

On Windows the model server variant is chosen at first launch: CUDA when an NVIDIA driver is present (with the matching CUDA runtime downloaded alongside), Vulkan when a Vulkan-capable driver exists, otherwise CPU. A GPU build that fails to start pins the CPU build (`%LOCALAPPDATA%\ai.orbixcore.app\.llama-cpu-fallback`); delete that file or set `ORBIX_LLAMA_VARIANT` to override.

**Linux.** Tauri's own prerequisites (`webkit2gtk-4.1`, `libayatana-appindicator3`, `librsvg2`, `patchelf`, `build-essential`, `pkg-config`), Rust, Node.js. The same `npm run tauri dev` is expected to work once the platform layer exists.

## Windows: merged, what is left

The Windows platform layer was developed in a separate tree from May to September 2026 and merged into this repository on 2026-09-17 as `cfg`-gated code next to the macOS implementations. It covers voice (SAPI), app control and notifications (PowerShell, Win32, WinRT toasts under a registered AppUserModelID), window reading and field automation (UI Automation, including `read_app_window` and browser tab titles), screen capture, the DPAPI master key, per-machine GPU selection for `llama-server`, the shell tool on `cmd.exe`, connector activation without symlinks, and NSIS/MSI packaging. CI builds and tests on `windows-latest`; the release workflow produces the installers with checksums.

Still open:

- The Outlook bridge over COM is written (`recent_messages`, `unread_count`, `search_messages`, `upcoming_events`, `today_events`) and its scripts are checked against a mocked object model, but it has not yet run against a live classic Outlook profile: the development PC only has the new Outlook. Things to confirm on a machine with classic Outlook signed in: the locale-formatted date filter on recurring events, the DASL search filter, and whether Outlook's programmatic-access guard prompts when sender addresses are read (it should not while antivirus is current).
- More Windows bridges. Any app with a COM object model fits the same manifest shape (`windows_prog_id`, `windows_prelude`, `windows_probe`, `windows_script`).
- Authenticode code signing: the release workflow is wired (`WINDOWS_SIGN_COMMAND`, see `orbixcore/src-tauri/scripts/SIGNING.md`) and verifies signatures when it is on, but no certificate is enrolled yet, so releases are unsigned and SmartScreen warns.
- Voice quality: SAPI is the system voice; Kokoro parity with macOS is untested on Windows.

### How the Outlook bridge behaves on Windows

- **Classic Outlook installed** (the `Outlook.Application` COM class is registered): `app_bridge` and `list_app_bridges` are offered to the model, the APP ACCESS digest lists Outlook as bridged, and reads run as PowerShell against the COM object model, across every mail store in the profile. A closed Outlook is started by COM in the background on first use. No account setup and no permission prompt; COM does need both processes at the same elevation, so an Outlook started as administrator cannot be read and the error says so.
- **Only the new Outlook installed** (`olk.exe`, no COM class): the bridge shows as Limited in Settings, the `app_bridge` tools are not advertised at all (advertising them was a dead path for the model, field failure 2026-09-16), and mail and calendar go through the connected Microsoft account.
- **A Microsoft account connected**: Outlook actions are served from Graph first on every platform, the local script is the no-account fallback.

## The Linux plan

Linux starts after Windows is supported. In order:

1. Build: confirm `cargo check` and `npm run tauri dev` on Ubuntu with the Tauri prerequisites; gate anything that fails.
2. Inference and speech: the `llama-server` Linux build (CPU first, Vulkan next), whisper.cpp as on the other platforms, Kokoro plus speech-dispatcher or Piper for the system voice.
3. Master key: Secret Service through `libsecret`, with a clear error when no keyring daemon is running.
4. System integration: notifications and app control over D-Bus, clipboard through `wl-clipboard` and `xclip`, window reading through AT-SPI2, screenshots through the desktop portal.
5. App bridges: none at first; the tools say so.
6. Packaging: AppImage and `.deb` from the release workflow, with checksums.
7. CI: `ubuntu-latest` job and release matrix entry.

Each step is a normal pull request against this repository, gated the same way as everything else.

## Reporting platform issues

Use the bug report template and pick the platform. Anything that works on macOS and not on another platform is a bug in that platform's layer, not a feature request, and is prioritised as one.
