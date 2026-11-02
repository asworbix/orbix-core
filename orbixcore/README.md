# orbixcore

The Tauri desktop app: React + TypeScript front end in `src/`, Rust backend in `src-tauri/src/`.

Start here: the [project README](../README.md), [docs/architecture.md](../docs/architecture.md), and [CONTRIBUTING.md](../CONTRIBUTING.md).

```bash
npm install
npm run tauri dev          # run the app against the dev server
npm run tauri:build        # release bundle (stages the CLI and speech helper first)

cd src-tauri && cargo test --bin orbixcore   # backend tests
npx tsc --noEmit && npx vite build           # front end
```
