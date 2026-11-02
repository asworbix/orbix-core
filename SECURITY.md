# Security

## Supported versions

| Version | Supported |
|---|---|
| 0.1.x (Developer Preview) | Yes, latest release only |
| Anything older | No |

## Reporting a vulnerability

Report privately. Do not open a public issue, discussion, or pull request for a security problem.

- Preferred: GitHub's private vulnerability reporting on this repository (Security tab > Report a vulnerability).
- Or email asw@orbixcore.ai.

Include what you found, how to reproduce it, and what you think the impact is. You will get an acknowledgement within 3 working days and a first assessment within 10. We will keep you informed while we fix it, credit you in the release notes if you want, and coordinate the public disclosure with you. Please give us 90 days before publishing details.

Do not post: credentials you found, exploit code against other people's installs, or personal data from any system.

## Trust model

Orbix Core is local-first. The design goal is that nothing leaves your machine unless you opt in, and when it does it passes one audited door. [docs/privacy.md](docs/privacy.md) lists every exit.

- **Local by default.** The LLM (Gemma 4 or Qwen 3 via llama.cpp), speech-to-text (whisper.cpp), and text-to-speech (Kokoro) all run on-device. No account or network is required to use the core app.

- **Secrets in the OS keychain.** API keys and connector credentials are encrypted with AES-256-GCM under a master key stored in the macOS Keychain (`storage.rs`). They are never written to disk in plaintext.

- **One egress door.** Every web search, web fetch, and cloud model call funnels through the egress gateway (`egress.rs`), which applies the workspace `CompliancePolicy` (residency and boundary gates) and the secure-relay redaction engine before anything leaves, and records the decision in a hash-chained audit log. A fresh install ships a strict default policy (compliance mode on, `RedactionMode::Strict`, residency `hard_gate` on).

- **Sandboxed, signed connectors.** Bob-authored and user-added connectors run as Deno subprocesses with kernel-enforced allow-lists (`--allow-net` pinned to declared hosts, `--allow-env` pinned to declared keys, `--allow-read` scoped to the module directory). A static AST scanner refuses modules that reach beyond their manifest, and every module's Ed25519 signature is verified at load; unsigned or tampered modules are refused unless the developer override `ORBIX_ALLOW_UNSIGNED_MODULES` is set.

- **Confidential folders.** Path-scoped ACLs (`data_control.rs`) are enforced on every file tool (read, write, list, edit).

- **Scoped autonomy.** Agent shifts see only the tools they were granted. Imagine, the free-rein mode, runs on a catalog with every sending, connecting, deleting, and automation-firing tool removed.

## Powerful tools: shell and browser

Two capabilities are intentionally powerful and gated:

- **`shell_exec`** runs an arbitrary shell command. It is approval-gated in the tool registry and RBAC-scoped, but a user who approves a command is trusting it. Treat it like a terminal: only approve commands you understand.

- **Browser driving** attaches to your real Chrome over the DevTools Protocol, which requires you to launch Chrome with `--remote-debugging-port=9222`. That flag exposes a control port to anything on localhost while Chrome runs; enable it only when you intend to let Orbix drive the browser.

## OAuth

Connectors use the standard native-app loopback OAuth flow: the provider redirects the authorization code to a `http://127.0.0.1:<port>/callback` URL that the app's local server reads. Each provider is your own registered OAuth app; Orbix ships no shared credentials (see `credentials.example.json`).

## Automated checks

CI runs gitleaks on every push and pull request, `cargo audit` on both Rust crates, `npm audit` on the app's runtime dependencies, and Dependabot keeps dependencies current. Release artifacts ship with a `SHA256SUMS.txt`.
