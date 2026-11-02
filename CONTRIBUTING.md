# Contributing to Orbix Core

Thanks for helping. This covers the repo layout, local setup, the checks every change must pass, and the conventions we hold to.

## Repo layout

```
orbixcore/            Tauri desktop app
  src/                React + TypeScript front end
  src-tauri/src/      Rust backend
  src-tauri/scripts/  Build scripts (stage the CLI and the Swift speech helper)
bob-cli/              The orbix terminal CLI crate (builds orbix and the bob alias)
browser-extension/    "Send to Orbix" browser extension
docs/                 Architecture and privacy documentation
scripts/              Repository tooling
```

The iOS companion and the OAuth relay live in private repositories for now.

## Platforms

One codebase, three targets: macOS and Windows ship today, Linux comes next. Platform code is gated with `#[cfg(target_os = "...")]` inside the shared modules, returns the same JSON to the front end, and says plainly when a capability is not available on a platform. Never fork a platform into its own tree or branch. Status, the shared-versus-specific matrix, and the Windows and Linux work lists: [docs/platforms.md](docs/platforms.md).

## Development setup

Prerequisites: [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org) 18+, and the platform toolchain (Xcode command line tools on macOS; Visual Studio Build Tools 2022 with the C++ workload on Windows). Per-platform notes are in [docs/platforms.md](docs/platforms.md).

```bash
cd orbixcore
npm install
npm run tauri dev
```

The app fetches its model and sidecar binaries on demand at runtime; nothing large is committed. `npm run tauri:build` produces a release bundle and runs the staging scripts first (`src-tauri/scripts/build-bob.sh`, `build-swift.sh`).

## Credentials for OAuth work

Orbix ships no shared credentials. If you work on a connector that needs OAuth:

```bash
cp orbixcore/src-tauri/credentials.example.json orbixcore/src-tauri/credentials.json
```

Fill in the client IDs for your own registered provider apps. `credentials.json` is gitignored and must never be committed. Never paste a real client secret into tracked source.

## Running the checks

Backend (a fresh clone needs one `npm run build` in `orbixcore` first: `tauri::generate_context!` refuses a missing `dist`; `npm run tauri dev` does not):

```bash
cd orbixcore/src-tauri
cargo test --bin orbixcore
```

Tests run against a temporary data folder, never your real one; set `ORBIX_DATA_DIR` to point the app itself somewhere else.

Front end:

```bash
cd orbixcore
npx tsc --noEmit
npx vite build
```

CLI:

```bash
cd bob-cli
cargo test --bin orbix --bin bob
```

CI runs all of these plus a secret scan (gitleaks), dependency audits (`cargo audit`, `npm audit` on runtime dependencies), and a hygiene check for em dashes and personal paths. A pull request cannot merge until CI is green. Some integration tests are `#[ignore]` because they need a running llama-server or Chrome; run them explicitly with `-- --ignored` when relevant.

Formatting: the Rust code is not yet `rustfmt`-clean as a whole, so CI does not enforce it. Keep new code formatted (`cargo fmt` on the files you touch) and do not reformat files you are not otherwise changing.

## Code style

- **No em dashes.** Do not use the em dash character (U+2014) anywhere: code, comments, UI copy, docs, commit messages. Use a hyphen, a colon, or restructure the sentence. CI enforces this.
- **Build generic mechanisms**, not per-entity patches. If a fix only helps one connector, model, or vertical, it is usually the wrong shape.
- **Keep components lean.** A workflow is a saved sequence of actions; steps pass references and small results, not large payloads.
- **Reproduce before fixing.** Confirm the actual failure before proposing a fix, and say how you verified the change in the pull request.
- **Match the surrounding code** in naming, comment density, and idiom.
- **Wire it to Bob.** A new capability gets a tool Bob can call, not only a panel. The app must stay usable by voice alone.

## Branches, commits, and pull requests

- `main` is protected and always releasable. Work on a branch (`fix/...`, `feat/...`, `docs/...`) and open a pull request against `main`.
- Keep pull requests focused: one change, one reason. Large refactors get an issue or a discussion first.
- Write commits with a clear subject line that says what changed and why. No em dashes.
- **Sign off your commits** (`git commit -s`). By signing off you certify the [Developer Certificate of Origin](https://developercertificate.org/): that you wrote the change or have the right to submit it under the project's license. There is no CLA.
- Fill in the pull request template: what changed, how it was verified, the checklist.
- A maintainer reviews every pull request (see `.github/CODEOWNERS`). Expect questions; they are about the change, not about you.

## Reporting issues

Use the bug report template. Include your Orbix Core version, Mac (chip, memory, macOS version), the model in use, exact steps, and what you saw. Strip anything private from logs before pasting. Reproducible reports get fixed first.

Security problems are never public issues: see [SECURITY.md](SECURITY.md).

## Proposing features

Open a discussion or a feature request describing the problem first, then the proposal. If Bob should be able to do it by voice, write down what you would say. Small proposals can go straight to a pull request; anything that changes on-disk formats, the tool catalog, or the egress rules needs agreement before code.

## Connector security model

Connector modules are sandboxed Deno programs. Every module:

- Declares a `manifest.json` with `host_allowlist` and `env_allowlist`; Deno enforces these at the OS level (`--allow-net`, `--allow-env`).
- Passes a static AST scan at activation: any host or env key reached in `module.ts` that is not declared in the manifest fails activation.
- Must be signed. The runtime refuses to load an unsigned or tampered module (the developer override `ORBIX_ALLOW_UNSIGNED_MODULES=1` ships off).

When adding or changing connector-authoring code, keep these invariants intact.

## Bundled binaries and licenses

The `deno` sidecar and `llama-server` binary are fetched at build or first-run time; they are not committed and carry their own licenses. See [orbixcore/src-tauri/THIRD_PARTY.md](orbixcore/src-tauri/THIRD_PARTY.md).
