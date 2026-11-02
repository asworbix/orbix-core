# Send to Orbix (browser extension)

Select a plan in Claude.ai or Microsoft Copilot and ship it to Orbix
Core with one click. Orbix executes it as a durable Run on the free
local model - the cloud does one planning turn, your Mac does the rest.

## Install (unpacked)

1. In Orbix Core: Settings > Models > turn on "Expose local model
   endpoint" and copy the token.
2. Chrome: `chrome://extensions` > enable Developer mode > Load
   unpacked > pick this `browser-extension/` folder.
3. Open the extension's Options, paste the token, Save, Test
   connection.

Store submission is a separate, later decision; unpacked is the
supported path for now.

## Use

- Select text on claude.ai or copilot.microsoft.com. A "Send to
  Orbix" pill appears bottom right (or right-click > Send to Orbix).
- If the selection is a JSON HandoffSpec (see docs/orbix-handoff.md,
  fenced or bare), it is sent as-is.
- Otherwise the selection becomes a plan: numbered or bulleted lines
  turn into one prompt step each; anything else runs as a single
  prompt step.
- The toast shows accepted or the exact reason it failed. Progress
  lives in the Orbix app's Run surfaces.

## Privacy

The extension talks only to `http://127.0.0.1:51231` (your Mac). The
token is stored in Chrome extension storage. Nothing is sent anywhere
else.
