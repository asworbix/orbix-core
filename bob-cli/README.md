# orbix / bob - Orbix's local-first agent as a CLI

`orbix` (alias `bob`) asks the Bob that runs in the Orbix Core app, from
any terminal. You `cd` into a project and type `bob "<what you want>"`:
Bob works in that folder with everything he has in the app - your
memory, agents, workflows, connected services and paired apps - and
streams what he is doing as he does it. It is one Bob: what you ask
here shows up in the app's chat as your own message. No cloud and
no account needed for local work.

## Install

```bash
cd bob-cli
./install.sh           # builds release + drops orbix (and bob) into ~/.local/bin
```

Make sure `~/.local/bin` is on your `PATH` (the installer warns if
it isn't).

## Run

```bash
bob "what's in this directory"
bob "add a docstring to every function in src/lib.rs"
bob "fix the cargo warnings in plugins.rs"
```

If Orbix Core is not running, `bob` starts it in the background first
(headless: no window, no Dock icon), so it works from a cold start.
Ctrl-C stops Bob's turn in the app, not just the terminal.

REPL mode (no prompt arg, attached terminal):

```bash
bob
→ list the rust files
→ what does main.rs do
→ :q
```

## Connect to another device (chat / connect)

Beyond the local agent loop, `orbix` can chat with an Orbix host running on a
different machine over your LAN - the host exposes an OpenAI-compatible
endpoint (Settings → Models → Expose local model endpoint) and you reach it
from anywhere on the network:

```bash
orbix connect                      # discover hosts, pick one, chat (no IP)
orbix chat --endpoint URL --token T   # or point at one directly
```

`connect` browses for hosts advertising `_orbix._tcp` over mDNS and lists
them; the token is never broadcast, so you supply it once (`--save` to
remember). Inside chat, `/connect` switches host. On Windows this is the
primary use - see [WINDOWS.md](WINDOWS.md).

## What Bob can do

Everything he can do in the app. In this folder specifically: shell
commands run here and relative file paths resolve here for the turn
(ahead of the Workspace setting in the app).

## Flags

```
bob [--verbose] [PROMPT...]
```

| flag | notes |
|---|---|
| `--verbose` | print every raw event the app streams |
| `--endpoint` | model server for `status` / `models` (default `http://127.0.0.1:8847`, or `LLAMA_ENDPOINT`) |
| `--model` | model `orbix chat` sends; Bob uses the model selected in the app |

## Security note

Bob acts in your current folder with the same permissions he has in
the app. The app's loopback `/cli` server only accepts connections
from this machine.

## Why a separate crate

Keeps the CLI free of Tauri's macOS-only system deps so it cross-
compiles cleanly and ships as one static binary. It holds no agent
logic of its own: the turn runs in the app (`/cli/peer/ask` in
`src-tauri/src/mobile_api.rs`, scoped by `turn_scope.rs`).
