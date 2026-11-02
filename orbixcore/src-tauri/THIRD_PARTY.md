# Third-Party Components

Orbix Core bundles or downloads several third-party components at build or first-run time. They are not committed to this repository; the build scripts under `scripts/` fetch and stage them, and the app downloads models on demand. Each keeps its own license.

## Binaries fetched or staged at build time

| Component | Role | License | Source |
|---|---|---|---|
| Deno | Sandbox runtime for connector modules | MIT | https://github.com/denoland/deno |
| llama.cpp / llama-server | Local LLM inference server (Metal on macOS; CPU, CUDA, or Vulkan build on Windows, picked per machine) | MIT | https://github.com/ggml-org/llama.cpp |
| NVIDIA CUDA runtime (cudart, cuBLAS DLLs) | Downloaded on Windows only, alongside the CUDA build of llama-server, when an NVIDIA driver is present | NVIDIA CUDA EULA (redistributable runtime) | Shipped with each llama.cpp release |

`scripts/download-deno.sh` stages the Deno sidecar; `scripts/build-bob.sh` builds the `orbix`/`bob` CLI. The `llama-server` binary is fetched on first use by the app.

## Models downloaded on first use

| Component | Role | License | Source |
|---|---|---|---|
| Gemma 4 (GGUF, via unsloth) | Default local chat/agent model family, with vision projectors | Gemma Terms of Use | https://huggingface.co/unsloth |
| Qwen 3.5 / 3.6 (GGUF, via unsloth) | Alternative local chat/agent model family, with vision projectors | Apache-2.0 | https://huggingface.co/unsloth |
| Qwen3 Embedding 0.6B (GGUF) | Embeddings for search and memory | Apache-2.0 | https://huggingface.co/Qwen/Qwen3-Embedding-0.6B-GGUF |
| whisper.cpp models (ggml `tiny`/`base`/`small` en) | Local speech-to-text | MIT | https://huggingface.co/ggerganov/whisper.cpp |
| Kokoro | Local neural text-to-speech | Apache-2.0 | Kokoro TTS |
| BGE / Nomic embedding models (optional) | Semantic search | MIT / Apache-2.0 | HuggingFace |

Users may add any other GGUF model from a HuggingFace repo; those models carry the license of their respective publishers.

## Language dependencies

- Rust crate licenses are recorded in `Cargo.lock` (and each crate's own license metadata).
- npm package licenses are recorded in `../package-lock.json`.

Run `cargo license` or an equivalent npm license tool to produce a full dependency license report.

## Notes

- Gemma is distributed under Google's Gemma Terms of Use rather than a standard OSI license; review those terms before redistribution.
- No third-party credentials, API keys, or OAuth client secrets are bundled. Users supply their own via `credentials.json` (gitignored); see `credentials.example.json`.
