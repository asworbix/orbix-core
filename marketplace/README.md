# Marketplace

Things people made for Orbix Core: skills, workflows, paired apps, and
connectors. Everything here is free, reviewed in a pull request, and listed
on [orbixcore.ai/marketplace](https://orbixcore.ai/marketplace).

## What goes where

| Folder | One item is | Installed by |
|---|---|---|
| `skills/` | `<slug>.md`: frontmatter `name`, `description`, `author`, then the instructions | Saving the file into the app's `skills` folder |
| `workflows/` | `<slug>.md`: frontmatter `name`, `description`, `author`, optional `schedule`, then the workflow as plain sentences | Pasting the sentences to Bob or into the Build tab |
| `paired-apps/` | `<slug>.json`: a peer recipe (same format as `orbixcore/src-tauri/resources/peers/`), plus a `description` and `author` | Saving the file into the app's `peers` folder |
| `connectors/` | `<slug>/<version>/`: `manifest.json`, `module.ts`, a `README.md` | Connectors, then +, then pointing at the folder; the app scans, signs, and activates it on your machine |

The app's data folder is `~/Library/Application Support/ai.orbixcore.app/`
on macOS and `%LOCALAPPDATA%\ai.orbixcore.app\` on Windows.

Skills, workflows, paired apps, and connectors that ship inside the app are
listed on the site too, marked as included. They come from the app's own
resources, not from this folder.

## Adding something

1. Add your item in the right folder. Use a short lowercase slug with
   dashes. Put your name or GitHub handle in `author`.
2. Run `node scripts/marketplace-index.mjs` and commit the updated
   `marketplace/index.json`. CI fails when the index is out of date.
3. Open a pull request with the DCO sign-off (`git commit -s`), as for any
   other change.

What gets merged: things that do what they say, ask for no more access than
they need, and contain no secrets. Connectors are read line by line; a
connector's `host_allowlist` is part of the review.
