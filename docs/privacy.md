# Privacy

This page says exactly what stays on your Mac, what can leave it, when, and how to stop it. It describes the code as it is today, not an aspiration. Where a claim only holds in some configurations, it says so.

## What stays local

Everything, unless one of the exits below is used:

- **The model.** Chat, tool calls, Imagine, agent shifts, workflow prompt steps: all run on `llama-server` bound to `127.0.0.1:8847` on this machine. The server never accepts connections from other hosts.
- **Speech.** Speech-to-text (whisper.cpp) and text-to-speech (Kokoro, plus the system speech helper) run on-device. Audio never leaves the machine.
- **Your data.** Conversations, memory, workflows, agents, skills, panels, the knowledge base, the audit log, generated documents, and everything the app reads from your files and apps stay on disk here.
- **Reading your apps.** Calendar, Mail, Outlook, Notes, Reminders, and Messages are read through their local scripting interfaces (macOS); windows are read through Accessibility on macOS and UI Automation on Windows. Nothing is uploaded.

No account is required. No analytics or telemetry module exists in the code base: `analytics.rs` computes dashboard numbers from local data and sends nothing. There is no crash reporter.

## What can leave the machine, and when

| Exit | When it happens | What is sent | How to prevent it |
|---|---|---|---|
| **Model and binary downloads** from `huggingface.co` and GitHub releases | When you download a model or projector, and on first use for `llama-server` (on Windows also the CUDA runtime when an NVIDIA GPU is detected) and the Deno sandbox binary | A plain HTTPS GET for the file; no account, no data of yours | Download nothing; the app is unusable without a model, so this one is effectively required once |
| **Web search** (DuckDuckGo) | When Bob, an agent, or a workflow step calls `web_search` | The search query, after the egress gateway's redaction pass | Do not ask for things that need the web; a workspace compliance policy can block the `web_search` egress kind outright |
| **Web fetch** | When a tool or step calls `web_fetch` with a URL | A GET to that URL, after the gateway's checks | Same as above; block the `web_fetch` egress kind |
| **Cloud inference** (`api.anthropic.com`, or Vertex AI in the EU when configured) | Only for an agent you created with provider `anthropic` and your own API key | The conversation slice for that agent turn, redacted by the gateway according to policy | Do not create a cloud agent; local is the default and needs no key |
| **Connected services** (Microsoft Graph, Google, GitHub, Slack, LinkedIn, and any connector you add) | Only after you connect a service with your own OAuth app, and only when a tool or workflow uses it | Whatever that connector's actions send: an email you asked Bob to send, a calendar event, an API call | Do not connect the service; disconnect it in Connectors to stop it; connector modules can only reach the hosts in their manifest |
| **Update check** (`orbixcore.ai/api/update`) | About four seconds after the app opens, and when you press Check for updates in Settings | Your platform (`darwin`), CPU architecture, and current app version in the URL; nothing else | There is no in-app switch yet. Blocking `orbixcore.ai` in a firewall or Little Snitch stops it; a setting is on the roadmap |
| **Browser driving** (your Chrome over the DevTools Protocol) | Only when you started Chrome with `--remote-debugging-port=9222` and asked Bob to use it | Nothing to us; Bob drives your browser locally, and the pages it visits see what any visit sees | Do not start Chrome with the flag |

Anything else is a bug. If you find traffic that this table does not explain, report it (see [SECURITY.md](../SECURITY.md)).

## The egress gateway

Web search, web fetch, and cloud inference share one door: `egress.rs`. Before a request leaves, the gateway applies the workspace compliance policy (data residency and boundary gates), runs the redaction engine over the outgoing text (PII stripping), and records the decision (allowed or blocked, and why) in the tamper-evident audit log. A fresh install ships strict: compliance mode on, redaction strict, residency hard-gated. Connector modules do not pass through the gateway; instead the Deno sandbox pins them to the hosts declared in their manifest at the operating-system level.

## What model providers can receive

- **Local models (default):** nothing. The model runs in a process on your Mac with no network access to anyone.
- **Anthropic or Vertex AI (opt-in cloud agent):** the redacted messages of that agent's turn, under your own API key and that provider's terms. The gateway's audit log records each call.

## Where data is stored

Everything lives under the app data folder: `~/Library/Application Support/ai.orbixcore.app/` on macOS, `%LOCALAPPDATA%\ai.orbixcore.app\` on Windows (Linux, when it lands, `~/.local/share/ai.orbixcore.app/`):

| Location | Contents |
|---|---|
| `models/`, `bin/` | Downloaded models and projectors, `llama-server`, `deno` |
| the base folder and `profiles/<id>/` | Your workspace: `conversations/`, workflows, agents, memory, skills, panels, the audit log, settings |
| encrypted files in the same tree | Connector credentials, the module signing key |
| macOS Keychain (service `ai.orbixcore.app`, account `master-key`); on Windows a DPAPI-wrapped key file in the data folder, readable only by that Windows user | The 32-byte master key that encrypts the files above |
| `~/Documents/Orbix Documents/` | Documents, sites, and app pages Bob creates (`Imagine/` for Imagine's loose files) |

## How data is encrypted

- **Secrets** (API keys, connector tokens, the signing key) are encrypted with AES-256-GCM under a per-install master key generated on first run. On macOS the key lives in the Keychain; on Windows it is wrapped with DPAPI (`CryptProtectData`, per-user scope) and stored in the data folder. Secrets are never written to disk in plaintext.
- **Everything else** (conversations, workflows, memory, documents) is stored as ordinary files. Protect them the way you protect the rest of your home folder: FileVault on, a login password, and a screen lock.
- **The audit log** is hash-chained: each entry commits to the previous one, so edits and deletions are detectable.

## How to delete everything

1. Quit Orbix Core.
2. Delete the app data folder (`~/Library/Application Support/ai.orbixcore.app/` on macOS, `%LOCALAPPDATA%\ai.orbixcore.app\` on Windows). Models are inside; this can be tens of gigabytes.
3. Delete `~/Documents/Orbix Documents/` if you do not want the generated documents.
4. On macOS, remove the master key from Keychain Access (search for `ai.orbixcore.app`), or from a terminal (on Windows the wrapped key was inside the folder deleted in step 2):
   `security delete-generic-password -s ai.orbixcore.app -a master-key`
5. Revoke the OAuth grants you gave in each provider's account settings (Microsoft, Google, GitHub, Slack, LinkedIn). Disconnecting in the app deletes the local token; revoking at the provider invalidates it.

Deleting a single workspace profile removes that profile's folder and nothing else. Individual conversations, memories, workflows, and documents can be deleted from the app.

## What Orbix Core itself collects

Nothing automatically. The only data that reaches `orbixcore.ai` is what the table above lists, each time triggered by an action you take: signing in (credentials), activating a license (the key and a machine identifier), and the update check (platform, architecture, version). The update check is the only one of these that runs without you pressing something, and only in an app that is signed in.

## Claims we do not make

We do not say "100% private", because a Mac with a connected mail account and a cloud agent is not. We say: local by default, every exit listed here, every exit under your control, and the gateway logs the ones that carry your text.
