# Pair mode

Drag the pair handle on Orbix Core's right edge onto any other app's window.
Bob works out how to talk to that app and says, in one line, what he can do
with it. That is the whole user-facing feature. This document is the
contract underneath it, for people extending it.

## What the user sees

- Sharing mode: the ports button in the title bar (next to the gear) turns
  it on. The window grows an animated pink-to-orange frame with a port on
  each edge and the hint "Drag an edge onto an app to connect". Drag a
  port onto an app window; the target glows; on release a chip appears
  under the greeting: `ChatGPT · Connected`. The mode stays on until the
  button is pressed again, so several apps can be connected in a row.
- Bob's one line in chat: "ChatGPT paired. I can type messages into ChatGPT
  and read what it shows."
- If one human step remains (`Restart Claude`, `Allow Accessibility`,
  `Check again`), it shows in a bar above the chat that stays until the
  step is done, with the button on it. The toast only announces it. Bob
  sees the same list under WAITING ON THE USER in his prompt and runs the
  step through `run_user_action` when the user says so in chat or by voice.
- By voice or chat: "pair with ChatGPT" does the same through `pair_app`.

## The ladder Bob runs on a drop

1. Identify the app from its pid: executable path, `.app` bundle,
   `Info.plist` (bundle id, name, whether it has an AppleScript dictionary).
2. Look up a recipe (`src-tauri/resources/peers/*.json`, user overrides in
   `<data_dir>/peers/`). A recipe names the app's bundle ids and its
   preferred transports. No recipe: bridge if an `app_bridge` manifest
   exists, then accessibility, then clipboard.
3. Try each transport, reads only:
   - `service`: the cloud service behind the app (recipe field `service`:
     microsoft, google, github, slack, linkedin). Connected already: Bob
     gets the real data through `api_call` with that connector. Not yet:
     one step in the "needs you" bar, `Sign in to Slack`, runs the OAuth
     flow and the pairing upgrades itself. Cross-platform by construction:
     recipes match bundle ids on macOS and process names on Windows, and
     the rung itself is HTTP.
   - `url`: the app's registered URL schemes (Info.plist on macOS, the
     HKCR protocol handlers whose open command runs the exe on Windows;
     http/https/file and friends filtered out). Recipes carry the link
     grammar as `url_actions` with `{{arg}}` templates; shipped ones
     cover Slack, Teams, Outlook, Mail, Cursor, VS Code, Obsidian, Zoom
     and Spotify. `peer_open` renders a template with URL-encoded args
     and opens it, refusing any URL whose scheme the app did not register.
     An app with a scheme but no known commands says so; `add_url_action`
     teaches one, kept untested until it opens cleanly.
   - `mcp_server`: an MCP server the recipe says the app or its service
     publishes (`mcp_server`: stdio command or HTTP url, optionally
     `bearer_from_service` so the user's sign-in is the token). Orbix is
     the client here: the server is added to the MCP panel, started, and
     its tools become a peer capability. Bob runs them with `mcp_tools`
     and `mcp_call`; the same client serves any server added by hand in
     the panel. Shipped: GitHub Desktop points at GitHub's remote MCP
     server with the GitHub sign-in as bearer (experimental).
   - `mcp`: write an `orbix` entry into the app's MCP config so the app
     spawns `orbix mcp --client <App>` and can call Bob. Other entries are
     untouched; a one-time `.orbix-backup` of the file is kept. Claude
     Desktop needs a restart to notice; Cursor does not.
   - `bridge`: an existing scripting manifest for the app.
   - `devtools`: a Chromium or Electron shell (detected by the v8 context
     snapshot the framework ships) driven over the DevTools protocol on a
     stable per-app port. Not running with the port open yet: one step in
     the "needs you" bar, `Relaunch ChatGPT with DevTools`, quits and
     reopens the app with `--remote-debugging-port`. Then `peer_send`
     types into the composer with insertText (contenteditable-safe) and
     presses Send, and `peer_read` returns the latest reply. Bob finds the
     selectors himself by looking at the page (`devtools_discover.rs`):
     the text input that reads like a message box, the Send-like button
     nearest to it, the most specific repeated marker for messages, each
     turned into a selector that survives reloads (id, data attribute,
     aria-label, else a short path). Recipes may ship selectors as hints;
     when one stops matching, `peer_send` and `peer_read` look again and
     keep what the page shows, so a markup change costs one retry.
   - `accessibility`: probe the front window for readable text and a
     message input. Asks for the Accessibility permission once if missing.
     Beyond the composer, `peer_controls` lists the window's buttons and
     fields from a fresh profile, and `peer_press` / `peer_fill` use them
     by label, re-profiling once when the window changed. Same profiles
     drive UI Automation on Windows.
   - `clipboard`: always works; only ever a fallback.
4. Save the peer (`<data_dir>/peers.json`) and announce it.
5. If the app publishes a scripting dictionary and has no bridge yet, learn
   it in the background (`bridge_author.rs`): `sdef` the bundle, summarise
   the dictionary, have the local model draft an app_bridge manifest with a
   probe and a few read actions, refuse any script that is not read-only
   (delete, make, save, open, close, keystrokes, shell, property writes),
   run the probe and every action against the live app, keep only the
   actions that answered, and save the manifest to `<data_dir>/bridges/`.
   One retry with the failures fed back. The peer is then refreshed and
   Bob says what he can read. Bob can also be asked directly: `learn_app`.

Bob's line is composed from what actually worked. A clipboard-only pairing
says so; it never promises a full connection.

## Bob's tools

| Tool | Does |
|---|---|
| `pair_app {app}` | Run the ladder for a running app by name. |
| `list_paired_apps` | Peers and Bob's line for each. |
| `peer_send {app, text}` | Type into the app's message input and send, or copy for the user to paste. |
| `peer_read {app}` | The app's front-window text. |
| `unpair_app {app}` | Remove the peer and undo the MCP entry. |
| `run_user_action {id}` | Do a step that was waiting on the user (restart, permission pane, re-check), by id or app name. |
| `learn_app {app}` | Read a running app's scripting dictionary, draft and verify a read-only bridge, activate it. |
| `peer_controls {app}` | Buttons and fields in the app's front window. |
| `peer_press {app, control}` | Press a button by label. |
| `peer_fill {app, field, value}` | Fill a field by label. |
| `mcp_tools {server}` | Start an MCP server if needed and list its tools. |
| `mcp_call {server, tool, args}` | Run one tool on an MCP server. |
| `peer_open {app, action, args}` | Open or create something in a paired app through one of its link actions. |
| `add_url_action {app, id, description, template}` | Teach a paired app a link action from its URL scheme. |
| `api_call {method, url, body?, connector?}` | With `connector`, calls a connected service as the user: token attached, relative paths joined to the API base, one refresh-and-retry on an expired token. |

The system prompt carries a `PAIRED APPS` block so Bob knows what is paired
without a tool call.

## Orbix as an MCP server: `orbix mcp`

`orbix mcp [--app-url URL] [--client NAME]` serves MCP over stdio and
forwards each tool call to the app's loopback routes. Tools offered to the
host app:

| Tool | Route |
|---|---|
| `bob {message}` | `POST /cli/peer/ask` - a full Bob turn, shown in Orbix's chat as "<Client> asks: ..." |
| `orbix_status` | `GET /cli/peer/status` |
| `orbix_run_workflow {name}` | `POST /cli/peer/run-workflow` |
| `orbix_search_memory {query}` | `GET /cli/memory/search?q=` |

When Orbix is not running, every tool returns an `isError` result that says
so; the protocol itself never fails. Manual config for any MCP host:

```json
{ "mcpServers": { "orbix": { "command": "/path/to/orbix", "args": ["mcp", "--client", "MyApp"] } } }
```

## Steps waiting on the user

`user_actions.rs` aggregates every step only the user can take into one
list: `{id, source, subject, label, reason, action}`. Peers are the first
source (a peer's `follow_up`); permissions and connector re-auth can join
without UI changes. Commands: `user_actions_list`, `user_action_run`.
Events: `user-actions` `{pending}` after any change, plus `data-changed`
with reason `user-actions`.

## Events

`pair-event` `{stage, text, peer}` with stages `identifying`, `connecting`,
`connected`, `failed`, `cancelled`, `unpaired`, `updated`. `data-changed`
with reason `peers` follows any change to the peer list. The overlay window
(`pair-overlay`) receives `pair-drag` frames at about 60 Hz.

## Adding an app

Drop a recipe into `<data_dir>/peers/<app>.json`:

```json
{
  "app": "Slack",
  "bundle_ids": ["com.tinyspeck.slackmacgap"],
  "transports": ["accessibility", "clipboard"]
}
```

No rebuild. A recipe with an `mcp.config_path` gets the tools transport;
one with `mcp.manual` gets that sentence appended to Bob's line instead.
A recipe with `service` gets the service rung; shipped ones cover Slack,
Microsoft Teams, Outlook and GitHub Desktop. Bob's prompt lists every
connected service with its API base and the endpoints to try first.

## The five doors

An app decides what it exposes. Pairing knocks on each door it has, in
the recipe's order, on both platforms:

| Door | macOS | Windows |
|---|---|---|
| Service behind the app | bundle id → service | exe name → service |
| URL scheme | Info.plist URL types | HKCR protocol handlers |
| MCP: the app calls Bob | config file | config file |
| MCP: Bob calls the app's server | stdio or HTTP | stdio or HTTP |
| Scripting | AppleScript dictionary (learned) | COM via PowerShell (manifests) |
| DevTools (Chromium/Electron) | relaunch with a port | relaunch with a port |
| Window: read, type, press, fill | Accessibility | UI Automation |
| Clipboard | pbcopy | Get-Clipboard |

All five doors are in. The MCP client speaks stdio and streamable HTTP; the legacy SSE
transport and MCP's own OAuth flow are not implemented, so remote servers
need a bearer from a connected service or an `AUTHORIZATION` env entry.

## Known limits

- Drag-to-pair is macOS only. `pair_app` by name works everywhere the
  window tools do.
- An app that is running without a window pairs with a note asking for one;
  `Check again` re-runs the ladder. Accessibility trees for Chromium and
  Electron apps exist only while a window is open, and their message
  composer sits 25-35 levels deep, so the walk goes to 40.
- ChatGPT's desktop app (a Chromium shell, bundle `com.openai.codex`) ships
  a Chromium AppleScript dictionary. Whether it exposes windows to it is
  unverified; pairing uses Accessibility, where its composer shows up as a
  text area named "Work with ChatGPT".
- Accessibility rungs break when apps change their UI. `peer_refresh`
  re-runs the ladder, and every refresh probes a bridge before counting
  it. A learned bridge that stops answering (on refresh or on use) is
  re-learned in the background, at most once per half hour per app;
  nothing is overwritten unless the new draft passes the smoke test.
  Shipped manifests are never touched.
- Learning takes under a minute on a local 12B model (Finder: about 45
  seconds for a probe plus verified read actions, one attempt). The
  drafting call is a plain completion with thinking off and a compact
  example; Bob's persona prompt is not involved. Drafted scripts are only
  as good as the dictionary and the model; the smoke test keeps the bad
  ones out.
- Chromium and Electron composers sit 25-35 levels deep: Claude's is a text
  area described "Write your prompt to Claude", ChatGPT's "Work with
  ChatGPT". The 40-level walk finds both.
- Run accessibility diagnostics one at a time. Two clients walking the same
  app concurrently return inconsistent trees.
- DevTools selectors are discovered from the page and re-discovered when
  they fail; the shipped ones for ChatGPT and Claude are hints. Verified
  on a headless Chrome with a fake chat page (`live_devtools_discovery`),
  not yet on the real apps. Driving a consumer app this way is close to
  those apps' terms of use, which is why it is an opt-in step, never
  automatic.
