use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

// Engine configuration (W1.6). Compile-time defaults; every value is
// overridable at runtime via environment so nothing is hardwired into
// call sites: ORBIX_LLAMA_VERSION, ORBIX_LLAMA_PORT, ORBIX_AGENT_PORT,
// ORBIX_LLAMA_URL.
const DEFAULT_LLAMA_SERVER_VERSION: &str = "b9969"; // verified 2026-07-12: all launch flags present, same forms
const DATA_DIR_NAME: &str = "ai.orbixcore.app";
const DEFAULT_LLAMA_SERVER_PORT: u16 = 8847;
const DEFAULT_AGENT_SERVER_PORT: u16 = 8848;

pub fn llama_server_version() -> String {
    std::env::var("ORBIX_LLAMA_VERSION")
        .unwrap_or_else(|_| DEFAULT_LLAMA_SERVER_VERSION.to_string())
}

pub fn llama_port() -> u16 {
    std::env::var("ORBIX_LLAMA_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LLAMA_SERVER_PORT)
}

pub fn agent_port() -> u16 {
    std::env::var("ORBIX_AGENT_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_AGENT_SERVER_PORT)
}

/// Base URL of the reasoning server, e.g. "http://127.0.0.1:8847".
pub fn llama_base_url() -> String {
    format!("http://127.0.0.1:{}", llama_port())
}

/// The context window the running llama-server was launched with, in
/// tokens. Set at launch; 8192 before any launch (the smallest tier), so
/// a prompt budget computed from it is never optimistic.
static CURRENT_CTX_SIZE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(8192);

pub fn current_ctx_size() -> usize {
    CURRENT_CTX_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

/// Base URL of the agent server, e.g. "http://127.0.0.1:8848".
pub fn agent_base_url() -> String {
    format!("http://127.0.0.1:{}", agent_port())
}

/// CUDA toolkit line of the ggml-org Windows CUDA asset we pull, and of
/// its cudart companion zip. The b9969 release ships both 12.4 and 13.3:
/// 13.3 carries kernels for the newest GPUs (RTX 50xx / Blackwell needs
/// it) but requires an r580+ driver; 12.4 covers older drivers. Pick by
/// the installed driver's major version, defaulting to 12.4 when
/// nvidia-smi is unreadable (the CPU fallback still catches a bad pick).
#[cfg(target_os = "windows")]
fn win_cuda_line() -> &'static str {
    let driver_major = crate::process_ext::std_command("nvidia-smi.exe")
        .args(["--query-gpu=driver_version", "--format=csv,noheader"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .lines()
                .next()?
                .split('.')
                .next()?
                .parse::<u32>()
                .ok()
        });
    match driver_major {
        Some(major) if major >= 580 => "13.3",
        _ => "12.4",
    }
}

/// Which llama-server build variant to run on Windows: "cuda", "vulkan"
/// or "cpu". Resolution order:
///   1. ORBIX_LLAMA_VARIANT env override (cpu|cuda|vulkan)
///   2. persisted CPU fallback marker - written after a GPU build failed
///      to start, so one bad driver never wedges the app in a retry loop
///   3. driver detection
#[cfg(target_os = "windows")]
pub fn llama_variant() -> String {
    if let Ok(v) = std::env::var("ORBIX_LLAMA_VARIANT") {
        let v = v.to_lowercase();
        if ["cpu", "cuda", "vulkan"].contains(&v.as_str()) {
            return v;
        }
        eprintln!("[local_llm] ignoring unknown ORBIX_LLAMA_VARIANT={v}");
    }
    if variant_fallback_path().exists() {
        return "cpu".to_string();
    }
    detect_gpu_variant()
}

/// Marker that pins the CPU build after a GPU-build startup failure.
/// Delete the file (or set ORBIX_LLAMA_VARIANT) to retry the GPU path.
#[cfg(target_os = "windows")]
fn variant_fallback_path() -> PathBuf {
    data_dir().join(".llama-cpu-fallback")
}

/// Cheap driver-marker GPU detection - file checks only, no subprocess.
/// nvidia-smi.exe ships with every NVIDIA driver install; vulkan-1.dll
/// is installed by NVIDIA, AMD and Intel drivers alike (so Vulkan is
/// the catch-all GPU tier for non-NVIDIA machines).
#[cfg(target_os = "windows")]
fn detect_gpu_variant() -> String {
    let sys32 = std::env::var("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32"))
        .unwrap_or_else(|_| PathBuf::from(r"C:\Windows\System32"));
    if sys32.join("nvidia-smi.exe").exists() || sys32.join("nvml.dll").exists() {
        return "cuda".to_string();
    }
    if sys32.join("vulkan-1.dll").exists() {
        return "vulkan".to_string();
    }
    "cpu".to_string()
}

/// Download URL for the llama-server release archive on this platform.
/// The macOS and Windows asset names are verified against the ggml-org
/// release layout; other platforms must set ORBIX_LLAMA_URL explicitly
/// rather than us guessing an asset name that may not exist.
pub fn llama_download_url() -> Result<String, String> {
    if let Ok(url) = std::env::var("ORBIX_LLAMA_URL") {
        return Ok(url);
    }
    let v = llama_server_version();
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let asset = format!("llama-{v}-bin-macos-arm64.tar.gz");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let asset = format!("llama-{v}-bin-macos-x64.tar.gz");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let asset = match llama_variant().as_str() {
        // CUDA needs its runtime DLLs too; ensure_llama_server pulls the
        // cudart companion zip (see cudart_download_url).
        "cuda" => format!("llama-{v}-bin-win-cuda-{}-x64.zip", win_cuda_line()),
        "vulkan" => format!("llama-{v}-bin-win-vulkan-x64.zip"),
        _ => format!("llama-{v}-bin-win-cpu-x64.zip"),
    };
    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    ))]
    {
        Ok(format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/{v}/{asset}"
        ))
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        let _ = v;
        Err(
            "No verified llama-server download for this platform. \
             Set ORBIX_LLAMA_URL to a llama.cpp release archive URL."
                .to_string(),
        )
    }
}

/// The CUDA runtime companion zip (cudart + cublas DLLs) that the CUDA
/// build of llama-server needs colocated with it. Lives in the same
/// release; its name is not version-prefixed.
#[cfg(target_os = "windows")]
fn cudart_download_url() -> String {
    let v = llama_server_version();
    format!(
        "https://github.com/ggml-org/llama.cpp/releases/download/{v}/cudart-llama-bin-win-cuda-{}-x64.zip",
        win_cuda_line()
    )
}

// ─────────────────────────────────────────────
//  Model roles - which model serves which purpose
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum ModelRole {
    Reasoning,  // Main chat, complex analysis, workflow design (26B-A4B)
    Agent,      // Fast routing, classification, background tasks (E4B)
}

/// The Gemma 4 Suite: two models that work together
pub const SUITE_REASONING_ID: &str = "gemma-4-27b";
pub const SUITE_AGENT_ID: &str = "gemma-4-e4b";

/// Get suite models (reasoning + agent)
pub fn suite_models() -> Vec<(&'static str, ModelRole)> {
    vec![
        (SUITE_AGENT_ID, ModelRole::Agent),        // Download first - smaller, usable immediately
        (SUITE_REASONING_ID, ModelRole::Reasoning), // Download second - larger
    ]
}

/// Check if the full Gemma 4 suite is downloaded
pub fn is_suite_ready() -> (bool, bool) {
    let models = list_models_with_status();
    let agent_ready = models.iter().any(|m| m.id == SUITE_AGENT_ID && m.downloaded);
    let reasoning_ready = models.iter().any(|m| m.id == SUITE_REASONING_ID && m.downloaded);
    (agent_ready, reasoning_ready)
}

// ─────────────────────────────────────────────
//  Model definitions
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct LocalModel {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    /// Computed at list time: false when the model cannot run in this
    /// Mac's memory (it would load, then OOM on the first token).
    #[serde(default = "default_true")]
    pub fits: bool,
    /// Computed at list time: great | ok | tight | too_big.
    #[serde(default)]
    pub fit_class: String,
    /// Model family: "gemma" or "qwen" for the curated catalog, empty for
    /// user-added files. The two families Orbix ships and verifies tool
    /// calling against; the UI groups by this.
    #[serde(default)]
    pub family: String,
    /// Computed at list time: the model can see images once its projector
    /// is on disk (every curated Gemma 4 and Qwen 3.x model is multimodal).
    #[serde(default)]
    pub vision: bool,
    /// Computed at list time: true for the curated catalog, false for
    /// user-added models (which the UI lets you remove from the list).
    #[serde(default)]
    pub builtin: bool,
    /// Computed at list time: the projector is on disk, so images work.
    #[serde(default)]
    pub vision_ready: bool,
}

fn default_true() -> bool {
    true
}

/// Total system memory in GB (16 assumed when detection fails).
pub fn total_memory_gb() -> f64 {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(16_000_000_000) as f64
            / 1_073_741_824.0
    }
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let bytes = unsafe {
            match GlobalMemoryStatusEx(&mut status) {
                Ok(()) => status.ullTotalPhys,
                Err(_) => 16_000_000_000,
            }
        };
        bytes as f64 / 1_073_741_824.0
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        16.0
    }
}

/// Rough memory-fit check: Metal's GPU working set on Apple Silicon is
/// ~70% of unified RAM, and a model needs its weights plus headroom for
/// KV cache and compute buffers (~15%). A model that fails this check
/// loads fine and then OOMs on the first token (field failure
/// 2026-08-03: a 20.6GB Qwen on a 24GB Mac produced only
/// 'No content in streaming response').
pub fn fits_in_memory(size_bytes: u64) -> bool {
    fit_class(size_bytes) != "too_big"
}

/// How comfortably a model runs on this Mac, by the ratio of what it
/// needs (weights x1.15 for KV + buffers) to what Metal can give it
/// (~70% of unified RAM): "great" (fast, lots of headroom), "ok"
/// (comfortable), "tight" (runs, slower, low headroom), "too_big"
/// (loads then OOMs - refused at launch).
pub fn fit_class(size_bytes: u64) -> &'static str {
    if size_bytes == 0 {
        // Custom models without a known size: no claim either way.
        return "unknown";
    }
    let usable_gb = total_memory_gb() * 0.70;
    let needed_gb = (size_bytes as f64 / 1_073_741_824.0) * 1.15;
    let ratio = needed_gb / usable_gb;
    if ratio <= 0.55 {
        "great"
    } else if ratio <= 0.80 {
        "ok"
    } else if ratio <= 1.0 {
        "tight"
    } else {
        "too_big"
    }
}

/// The best model for THIS Mac: first entry in the curated order
/// (flagship first) whose fit is comfortable. Never recommends "tight".
pub fn recommended_model_id() -> Option<String> {
    chat_models()
        .into_iter()
        .find(|m| matches!(fit_class(resident_bytes(&m)), "great" | "ok"))
        .map(|m| m.id)
}

/// One curated entry. Sizes are exact HF byte counts (HEAD-verified
/// 2026-09-06); `family` is what the UI groups by and what the tool-call
/// eval gate (research/tool-call-eval) runs per family.
fn curated(id: &str, name: &str, family: &str, description: &str, repo: &str, filename: &str, size_bytes: u64) -> LocalModel {
    LocalModel {
        id: id.to_string(),
        name: name.to_string(),
        description: description.to_string(),
        filename: filename.to_string(),
        url: format!("https://huggingface.co/{}/resolve/main/{}", repo, filename),
        size_bytes,
        downloaded: false,
        fits: true,
        fit_class: String::new(),
        family: family.to_string(),
        vision: false,
        builtin: true,
        vision_ready: false,
    }
}

/// The curated catalog: Gemma 4 and Qwen 3.x only.
///
/// Two families, on purpose. Every model here goes through the same
/// native tool-calling contract (OpenAI-style `tools`, `role: tool`
/// results, thinking off by default) and the same eval gate, so "Bob
/// works" means the same thing on each of them. Adding a third family
/// means adding it to the gate first. Order within a family is best
/// first; `recommended_model_id` walks this order and picks the first
/// model that runs comfortably on this device.
pub fn available_models() -> Vec<LocalModel> {
    vec![
        // ── Gemma 4 ──
        // UD-Q3_K_M: 26B MoE, 4B active per token, ~12.7GB on disk. Q3_K
        // handles MoE well (only the active experts matter per token);
        // the quality drop vs Q4_K_M is ~2%, the memory win is 4GB.
        curated("gemma-4-27b", "Gemma 4 26B-A4B", "gemma",
            "Recommended - MoE, 26B knowledge with 4B active per token. Fast, strong reasoning, vision. 24GB+ device.",
            "unsloth/gemma-4-26B-A4B-it-GGUF", "gemma-4-26B-A4B-it-UD-Q3_K_M.gguf", 12_728_497_888),
        curated("gemma-4-31b", "Gemma 4 31B", "gemma",
            "Dense flagship - best reasoning and coding, 256K context, vision. 32GB+ device.",
            "unsloth/gemma-4-31B-it-GGUF", "gemma-4-31B-it-Q4_K_M.gguf", 18_323_733_440),
        curated("gemma-4-12b", "Gemma 4 12B", "gemma",
            "Dense mid-size - strong general assistant with vision. Great on 16GB.",
            "unsloth/gemma-4-12b-it-GGUF", "gemma-4-12b-it-Q4_K_M.gguf", 7_121_861_440),
        curated("gemma-4-e4b", "Gemma 4 E4B", "gemma",
            "Compact - fast tool calling, vision, 140+ languages. Great on 8GB+.",
            "unsloth/gemma-4-E4B-it-GGUF", "gemma-4-E4B-it-Q4_K_M.gguf", 4_977_171_584),
        curated("gemma-4-e2b", "Gemma 4 E2B", "gemma",
            "Tiny - fastest replies, vision. Runs anywhere.",
            "unsloth/gemma-4-E2B-it-GGUF", "gemma-4-E2B-it-Q4_K_M.gguf", 3_106_738_272),
        // ── Qwen 3.x ──
        // Qwen3.6 MoE: 35B knowledge, 3B active per token. Sharpest tool
        // discipline in the loop; needs a 32GB device for the Q4 file.
        curated("qwen-3.6-35b-a3b", "Qwen3.6 35B-A3B", "qwen",
            "MoE - 35B knowledge with 3B active per token. Sharpest tool calling, vision. 32GB+ device.",
            "unsloth/Qwen3.6-35B-A3B-GGUF", "Qwen3.6-35B-A3B-UD-Q4_K_M.gguf", 22_134_528_992),
        curated("qwen-3.6-27b", "Qwen3.6 27B", "qwen",
            "Dense flagship - deep reasoning and coding, vision. 32GB+ device.",
            "unsloth/Qwen3.6-27B-GGUF", "Qwen3.6-27B-Q4_K_M.gguf", 16_817_244_384),
        curated("qwen-3.5-9b", "Qwen3.5 9B", "qwen",
            "Dense mid-size - precise tool calling, strong multilingual, vision. Great on 16GB.",
            "unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q4_K_M.gguf", 5_680_522_464),
        curated("qwen-3.5-4b", "Qwen3.5 4B", "qwen",
            "Compact - quick and capable, vision. Great on 8GB+.",
            "unsloth/Qwen3.5-4B-GGUF", "Qwen3.5-4B-Q4_K_M.gguf", 2_740_937_888),
        // ── Embeddings: for semantic memory and knowledge search. Runs on
        // an embeddings endpoint (--embeddings --pooling mean); the chat
        // models above do not produce good embeddings.
        curated("qwen3-embedding-0.6b", "Qwen3 Embedding 0.6B", "qwen",
            "Embeddings for search and memory - best-in-class small retriever.",
            "Qwen/Qwen3-Embedding-0.6B-GGUF", "Qwen3-Embedding-0.6B-Q8_0.gguf", 639_150_592),
    ]
}

/// Chat models only (the embedding model is not something Bob talks to).
pub fn chat_models() -> Vec<LocalModel> {
    available_models().into_iter().filter(|m| !is_embedding_model(&m.id)).collect()
}

pub fn is_embedding_model(id: &str) -> bool {
    id.contains("embedding")
}

// ─────────────────────────────────────────────
//  Vision (multimodal projector)
// ─────────────────────────────────────────────
//
// A vision model needs a second GGUF - the multimodal projector, "mmproj" -
// loaded alongside its weights (llama-server's --mmproj). The projector turns
// image pixels into tokens the model attends to; without it the model is
// text-only and drops any image you send. We key the projector off the model
// id rather than adding fields to LocalModel so custom_models.json keeps
// deserializing unchanged. A built-in model is "vision-capable" precisely
// when it has an entry here.

pub struct Mmproj {
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
}

/// The projector for a built-in vision model, or None for text-only models.
/// Local filenames are namespaced by model so projectors that share an
/// upstream name do not collide in the models folder.
pub fn mmproj_for(model_id: &str) -> Option<Mmproj> {
    // Every curated chat model is natively multimodal and ships an F16
    // projector in its own repo. (local filename, repo, size verified via
    // HF HEAD 2026-09-06.) Custom models have no projector entry.
    let (local, repo, size_bytes) = match model_id {
        "gemma-4-27b" => ("gemma-4-26B-A4B-mmproj-F16.gguf", "unsloth/gemma-4-26B-A4B-it-GGUF", 1_193_058_784u64),
        "gemma-4-31b" => ("gemma-4-31B-mmproj-F16.gguf", "unsloth/gemma-4-31B-it-GGUF", 1_198_957_024),
        "gemma-4-12b" => ("gemma-4-12b-mmproj-F16.gguf", "unsloth/gemma-4-12b-it-GGUF", 175_115_840),
        "gemma-4-e4b" => ("gemma-4-E4B-mmproj-F16.gguf", "unsloth/gemma-4-E4B-it-GGUF", 990_372_672),
        "gemma-4-e2b" => ("gemma-4-E2B-mmproj-F16.gguf", "unsloth/gemma-4-E2B-it-GGUF", 985_654_080),
        "qwen-3.6-35b-a3b" => ("Qwen3.6-35B-A3B-mmproj-F16.gguf", "unsloth/Qwen3.6-35B-A3B-GGUF", 899_283_680),
        "qwen-3.6-27b" => ("Qwen3.6-27B-mmproj-F16.gguf", "unsloth/Qwen3.6-27B-GGUF", 927_607_360),
        "qwen-3.5-9b" => ("Qwen3.5-9B-mmproj-F16.gguf", "unsloth/Qwen3.5-9B-GGUF", 918_166_080),
        "qwen-3.5-4b" => ("Qwen3.5-4B-mmproj-F16.gguf", "unsloth/Qwen3.5-4B-GGUF", 672_423_616),
        _ => return None,
    };
    Some(Mmproj {
        filename: local.to_string(),
        url: format!("https://huggingface.co/{}/resolve/main/mmproj-F16.gguf", repo),
        size_bytes,
    })
}

/// Whether a model can see images (has a projector we know how to fetch).
pub fn is_vision_model(model_id: &str) -> bool {
    mmproj_for(model_id).is_some()
}

/// Whether a vision model's projector is already on disk. False for text
/// models (nothing to download) and for vision models missing their mmproj.
pub fn mmproj_downloaded(model_id: &str) -> bool {
    mmproj_for(model_id)
        .map(|m| model_path(&m.filename).exists())
        .unwrap_or(false)
}

/// Optional embeddings endpoint for semantic search (W3). Set
/// ORBIX_EMBED_ENDPOINT to a llama-server running an embedding model with
/// --embeddings --pooling mean (e.g. http://127.0.0.1:8850). When unset,
/// callers fall back to keyword search.
pub fn embed_endpoint() -> Option<String> {
    std::env::var("ORBIX_EMBED_ENDPOINT").ok().filter(|s| !s.trim().is_empty())
}

// ─────────────────────────────────────────────
//  Paths
// ─────────────────────────────────────────────

pub fn data_dir() -> PathBuf {
    // One resolver for the app folder (storage::base_dir): ORBIX_DATA_DIR
    // and the test-suite temp folder apply to models and binaries too.
    crate::storage::base_dir()
}

pub fn models_dir() -> PathBuf {
    // Shared across workspace profiles - models are gigabytes.
    crate::storage::base_dir().join("models")
}

pub fn bin_dir() -> PathBuf {
    // Shared across workspace profiles.
    crate::storage::base_dir().join("bin")
}

fn llama_server_path() -> PathBuf {
    // The binary lives INSIDE its versioned extract folder, colocated with
    // its dylibs/DLLs. Never relocate it: that is exactly what broke the
    // b8753 -> b9969 update - the binary moved to a flat dir full of old
    // dylibs while its new libllama-server-impl.dylib stayed behind, and
    // every launch died in dyld before main().
    #[cfg(target_os = "windows")]
    {
        // The folder is keyed on version AND variant so a cpu install and
        // a cuda/vulkan install never mix DLLs - and a CPU fallback after
        // a GPU failure can reuse an already-downloaded cpu folder.
        bin_dir()
            .join(format!("llama-{}-{}", llama_server_version(), llama_variant()))
            .join("llama-server.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        bin_dir().join(format!("llama-{}", llama_server_version())).join("llama-server")
    }
}

pub fn model_path(filename: &str) -> PathBuf {
    models_dir().join(filename)
}

// ─────────────────────────────────────────────
//  Download progress tracking
// ─────────────────────────────────────────────

pub struct DownloadProgress {
    pub total_bytes: AtomicU64,
    pub downloaded_bytes: AtomicU64,
    pub is_downloading: AtomicBool,
    pub current_model_id: std::sync::Mutex<String>,
}

impl Default for DownloadProgress {
    fn default() -> Self {
        Self {
            total_bytes: AtomicU64::new(0),
            downloaded_bytes: AtomicU64::new(0),
            is_downloading: AtomicBool::new(false),
            current_model_id: std::sync::Mutex::new(String::new()),
        }
    }
}

// ─────────────────────────────────────────────
//  llama-server binary management
// ─────────────────────────────────────────────

fn llama_server_download_url() -> Result<String, String> {
    // Platform-aware + env-overridable (W1.6); see llama_download_url.
    llama_download_url()
}

pub fn is_server_binary_available() -> bool {
    llama_server_path().exists()
}

/// Check if the server binary is the expected version (matches llama_server_version()).
/// Returns false if the binary is outdated and should be re-downloaded.
fn is_server_current_version() -> bool {
    let path = llama_server_path();
    if !path.exists() { return false; }
    // Check if a version marker file exists with the current version
    let marker = bin_dir().join(".llama-server-version");
    if let Ok(v) = std::fs::read_to_string(&marker) {
        v.trim() == llama_server_version()
    } else {
        false // No marker = old binary, re-download
    }
}

/// Serialize llama-server installs. The reasoning server and the agent
/// server both call ensure_llama_server at startup; without this lock the
/// two extractions race in the same scratch dir ("tar.exe: could not
/// chdir to extract-tmp"), which on Windows even tripped the GPU->CPU
/// fallback for what was really a self-inflicted install race.
fn ensure_lock() -> &'static tokio::sync::Mutex<()> {
    static L: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    L.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[cfg(not(target_os = "windows"))]
pub async fn ensure_llama_server() -> Result<PathBuf, String> {
    let path = llama_server_path();

    // Re-download if binary is missing or outdated
    if path.exists() && is_server_current_version() {
        return Ok(path);
    }

    // One installer at a time; the loser of the race finds the binary
    // present and returns immediately.
    let _guard = ensure_lock().lock().await;
    if path.exists() && is_server_current_version() {
        return Ok(path);
    }

    // Clean old binary and libs before downloading new version
    if path.exists() {
        std::fs::remove_file(&path).ok();
    }

    std::fs::create_dir_all(bin_dir()).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    let url = llama_server_download_url()?;
    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }

    let archive_path = bin_dir().join("llama-server.tar.gz");
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    crate::storage::atomic_write(&archive_path, &bytes)
        .map_err(|e| format!("Failed to save archive: {}", e))?;

    // Extract from tar.gz
    let output = tokio::process::Command::new("tar")
        .arg("xzf")
        .arg(&archive_path)
        .arg("-C")
        .arg(bin_dir())
        .output()
        .await
        .map_err(|e| format!("Extract failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Extract failed: {}", stderr));
    }

    // The archive extracts to a versioned folder with the binary and ALL
    // of its dylibs colocated - run it from there. Only if the archive
    // layout ever changes do we go hunting and rename what we find into
    // the expected versioned folder.
    let bin = bin_dir();
    if !path.exists() {
        let find_output = tokio::process::Command::new("find")
            .arg(&bin)
            .arg("-name")
            .arg("llama-server")
            .arg("-type")
            .arg("f")
            .output()
            .await
            .map_err(|e| format!("Find failed: {}", e))?;
        let found = String::from_utf8_lossy(&find_output.stdout);
        let found_path = found
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && PathBuf::from(l) != bin.join("llama-server"));
        let Some(found_path) = found_path else {
            return Err("llama-server binary not found in archive".to_string());
        };
        let found_pb = PathBuf::from(found_path);
        let expected_dir = path.parent().unwrap_or(&bin).to_path_buf();
        match found_pb.parent() {
            Some(parent) if parent != bin => {
                // Folder layout with a different name: rename the whole
                // folder so binary and dylibs stay together.
                std::fs::rename(parent, &expected_dir)
                    .map_err(|e| format!("Failed to stage server folder: {}", e))?;
            }
            _ => {
                // Flat layout: everything extracted to bin root. Move the
                // binary into the versioned dir and copy the dylibs beside
                // it so the pair can never drift apart again.
                std::fs::create_dir_all(&expected_dir).ok();
                std::fs::rename(&found_pb, &path)
                    .map_err(|e| format!("Failed to move binary: {}", e))?;
                if let Ok(entries) = std::fs::read_dir(&bin) {
                    for entry in entries.flatten() {
                        let name = entry.file_name();
                        let n = name.to_string_lossy();
                        if n.ends_with(".dylib") || n.ends_with(".metal") || n.ends_with(".metallib") {
                            std::fs::copy(entry.path(), expected_dir.join(&name)).ok();
                        }
                    }
                }
            }
        }
    }

    // Remove any stale flat-layout binary so nothing can ever exec a
    // binary that is separated from its dylibs.
    let stale = bin.join("llama-server");
    if stale.exists() {
        std::fs::remove_file(&stale).ok();
    }

    // Make executable
    tokio::process::Command::new("chmod")
        .arg("+x")
        .arg(&path)
        .output()
        .await
        .ok();

    // Write version marker so we know this binary is current
    crate::storage::atomic_write(bin.join(".llama-server-version"), llama_server_version()).ok();

    // Clean up archive and extracted subdirs
    std::fs::remove_file(&archive_path).ok();
    std::fs::remove_dir_all(bin.join("build")).ok();

    Ok(path)
}

/// Windows: same versioned-folder discipline as macOS - llama-server.exe
/// stays colocated with the DLLs it shipped with, inside
/// bin/llama-{version}/. The release asset is a .zip; tar.exe (bundled
/// with Windows 10 1803+) extracts ZIPs natively.
#[cfg(target_os = "windows")]
pub async fn ensure_llama_server() -> Result<PathBuf, String> {
    let path = llama_server_path();

    // Re-download if binary is missing or outdated
    if path.exists() && is_server_current_version() {
        return Ok(path);
    }

    // One installer at a time; the loser of the race finds the binary
    // present and returns immediately.
    let _guard = ensure_lock().lock().await;
    if path.exists() && is_server_current_version() {
        return Ok(path);
    }

    let bin = bin_dir();
    std::fs::create_dir_all(&bin).map_err(|e| format!("Failed to create bin dir: {}", e))?;

    let url = llama_server_download_url()?;
    eprintln!(
        "[local_llm] installing llama-server {} ({} variant)",
        llama_server_version(),
        llama_variant()
    );
    let client = reqwest::Client::new();

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Download failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with status: {}", resp.status()));
    }

    let archive_path = bin.join("llama-server.zip");
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read download: {}", e))?;

    crate::storage::atomic_write(&archive_path, &bytes)
        .map_err(|e| format!("Failed to save archive: {}", e))?;

    // Extract into a scratch dir, then stage the folder holding
    // llama-server.exe (DLLs colocated) as the versioned folder.
    let extract_dir = bin.join("extract-tmp");
    std::fs::remove_dir_all(&extract_dir).ok();
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("Failed to create extract dir: {}", e))?;

    let output = crate::process_ext::tokio_command("tar.exe")
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&extract_dir)
        .output()
        .await
        .map_err(|e| format!("Extract failed (tar.exe missing - Windows 10 1803+ required): {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Extract failed: {}", stderr));
    }

    // The CUDA build ships without its runtime: pull the cudart companion
    // zip from the same release and extract it into the same scratch tree
    // so the DLL sweep below stages cudart/cublas next to the exe.
    if llama_variant() == "cuda" {
        let cudart_url = cudart_download_url();
        let resp = client
            .get(&cudart_url)
            .send()
            .await
            .map_err(|e| format!("CUDA runtime download failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("CUDA runtime download failed with status: {}", resp.status()));
        }
        let cudart_zip = bin.join("cudart.zip");
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read CUDA runtime download: {}", e))?;
        crate::storage::atomic_write(&cudart_zip, &bytes)
            .map_err(|e| format!("Failed to save CUDA runtime archive: {}", e))?;
        let out = crate::process_ext::tokio_command("tar.exe")
            .arg("-xf")
            .arg(&cudart_zip)
            .arg("-C")
            .arg(&extract_dir)
            .output()
            .await
            .map_err(|e| format!("CUDA runtime extract failed: {}", e))?;
        std::fs::remove_file(&cudart_zip).ok();
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!("CUDA runtime extract failed: {}", stderr));
        }
    }

    // Locate llama-server.exe wherever the archive put it.
    let mut found_server: Option<PathBuf> = None;
    for entry in walkdir::WalkDir::new(&extract_dir).max_depth(5) {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().is_file() { continue; }
        if entry.file_name().to_string_lossy().eq_ignore_ascii_case("llama-server.exe") {
            found_server = Some(entry.path().to_path_buf());
            break;
        }
    }
    let server_src = found_server
        .ok_or_else(|| "llama-server.exe not found in archive".to_string())?;
    let src_dir = server_src.parent().unwrap_or(&extract_dir).to_path_buf();

    // Stage every file colocated with the exe (that's where the DLLs are)
    // into the versioned folder, so the pair can never drift apart.
    let expected_dir = path.parent().unwrap_or(&bin).to_path_buf();
    std::fs::remove_dir_all(&expected_dir).ok();
    std::fs::create_dir_all(&expected_dir)
        .map_err(|e| format!("Failed to create server folder: {}", e))?;
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) { continue; }
            let dest = expected_dir.join(entry.file_name());
            if std::fs::rename(entry.path(), &dest).is_err() {
                std::fs::copy(entry.path(), &dest)
                    .map_err(|e| format!("Failed to stage {}: {}", entry.file_name().to_string_lossy(), e))?;
            }
        }
    }
    // Some archives scatter extra DLLs in sibling dirs; copy any we missed.
    for entry in walkdir::WalkDir::new(&extract_dir).max_depth(5) {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().is_file() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().ends_with(".dll") { continue; }
        let dest = expected_dir.join(&name);
        if !dest.exists() {
            std::fs::copy(entry.path(), &dest).ok();
        }
    }

    if !path.exists() {
        return Err("llama-server.exe was not staged into the versioned folder".to_string());
    }

    // Write version marker so we know this binary is current
    crate::storage::atomic_write(bin.join(".llama-server-version"), llama_server_version()).ok();

    // Clean up archive and scratch dir, plus any legacy un-suffixed
    // folder from before install dirs were keyed on variant.
    std::fs::remove_file(&archive_path).ok();
    std::fs::remove_dir_all(&extract_dir).ok();
    std::fs::remove_dir_all(bin.join(format!("llama-{}", llama_server_version()))).ok();

    // The pre-versioned layout left llama-server and its whole extracted
    // toolchain flat at the bin root (~1GB of stale exes/DLLs). Sweep the
    // known llama.cpp names; everything else in bin/ (e.g. the managed
    // deno.exe) stays.
    if let Ok(entries) = std::fs::read_dir(&bin) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let legacy = (name.starts_with("llama")
                && (name.ends_with(".exe") || name.ends_with(".dll")))
                || (name.starts_with("ggml") && name.ends_with(".dll"))
                || name == "mtmd.dll"
                || name == "rpc-server.exe"
                || (name.starts_with("libomp") && name.ends_with(".dll"));
            if legacy {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }

    Ok(path)
}

// ─────────────────────────────────────────────
//  Model downloading
// ─────────────────────────────────────────────

pub fn is_model_downloaded(filename: &str) -> bool {
    model_path(filename).exists()
}

/// Merge built-in and user-added custom models. Built-ins win on id
/// collisions. Pure so the resolution rules are unit-testable (W1.1).
fn merge_models(builtin: Vec<LocalModel>, custom: Vec<LocalModel>) -> Vec<LocalModel> {
    let mut models = builtin;
    let builtin_ids: std::collections::HashSet<String> =
        models.iter().map(|m| m.id.clone()).collect();
    for m in custom {
        if builtin_ids.contains(&m.id) { continue; }
        models.push(m);
    }
    models
}

/// Bytes that end up resident for a model: weights plus its vision
/// projector when it has one (the projector loads alongside the weights).
pub fn resident_bytes(m: &LocalModel) -> u64 {
    m.size_bytes + mmproj_for(&m.id).map(|mm| mm.size_bytes).unwrap_or(0)
}

pub fn list_models_with_status() -> Vec<LocalModel> {
    let builtin_ids: std::collections::HashSet<String> =
        available_models().iter().map(|m| m.id.clone()).collect();
    let mut models = merge_models(available_models(), load_custom_models());
    for m in models.iter_mut() {
        m.downloaded = is_model_downloaded(&m.filename);
        let resident = resident_bytes(m);
        m.fits = fits_in_memory(resident);
        m.fit_class = fit_class(resident).to_string();
        m.vision = is_vision_model(&m.id);
        m.vision_ready = m.vision && mmproj_downloaded(&m.id);
        m.builtin = builtin_ids.contains(&m.id);
    }
    models
}

/// GGUF files in the models folder that no catalog or custom entry
/// references: leftovers from model families Orbix no longer ships, or
/// aborted downloads. Returned so the UI can offer to reclaim the space;
/// nothing is deleted without the user asking.
pub fn unsupported_model_files() -> Vec<(String, u64)> {
    let mut known: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in merge_models(available_models(), load_custom_models()) {
        known.insert(m.filename.clone());
        if let Some(mm) = mmproj_for(&m.id) {
            known.insert(mm.filename);
        }
    }
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(models_dir()) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_model = name.ends_with(".gguf") || name.ends_with(".gguf.tmp") || name.ends_with(".gguf.part");
            if !is_model || known.contains(&name) {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push((name, size));
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// Delete one unsupported file by name. Refuses anything the catalog
/// references and any path component, so it can only ever remove a
/// leftover in the models folder.
pub fn delete_unsupported_model_file(name: &str) -> Result<(), String> {
    if name.contains('/') || name.contains("..") {
        return Err("Invalid file name.".to_string());
    }
    if !unsupported_model_files().iter().any(|(n, _)| n == name) {
        return Err(format!("{} is in use by a listed model.", name));
    }
    std::fs::remove_file(model_path(name)).map_err(|e| format!("Couldn't delete {}: {}", name, e))
}

/// Resolve a model id across built-in AND custom models (W1.1).
/// `select_local_model` previously searched only `available_models()`,
/// so selecting a user-added HF model silently failed to start the
/// server while `ensure_model_loaded` (which uses the merged list)
/// disagreed. Every id lookup must go through here.
pub fn find_model(id: &str) -> Option<LocalModel> {
    list_models_with_status().into_iter().find(|m| m.id == id)
}

// ─────────────────────────────────────────────
//  User-supplied (custom) models
// ─────────────────────────────────────────────
//
// Stored in `custom_models.json` alongside the rest of the orbix data.
// The user provides a display name and a GGUF URL; we derive an id and
// filename from the URL so the existing download/run machinery works
// without any changes downstream.

pub fn load_custom_models() -> Vec<LocalModel> {
    super::storage::load_json::<Vec<LocalModel>>("custom_models.json").unwrap_or_default()
}

pub fn save_custom_models(list: &[LocalModel]) -> Result<(), String> {
    super::storage::save_json("custom_models.json", &list.to_vec())
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

pub fn add_custom_model(
    name: String,
    url: String,
    description: Option<String>,
    size_hint: Option<u64>,
) -> Result<LocalModel, String> {
    let trimmed_url = url.trim().to_string();
    if !trimmed_url.starts_with("https://") {
        return Err("URL must start with https://".to_string());
    }
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Name is required".to_string());
    }
    if !trimmed_url.to_lowercase().ends_with(".gguf") {
        return Err("URL must point to a .gguf file".to_string());
    }

    // Derive a filename from the URL (last path segment) and a stable id
    // from the slugged display name.
    let filename = trimmed_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Could not parse filename from URL".to_string())?
        .to_string();

    let mut id = slugify(trimmed_name);
    if id.is_empty() {
        id = slugify(&filename.replace(".gguf", ""));
    }
    if id.is_empty() {
        return Err("Could not derive an id for this model".to_string());
    }

    let mut list = load_custom_models();
    if list.iter().any(|m| m.id == id) {
        return Err(format!("A custom model with id '{}' already exists", id));
    }
    let model = LocalModel {
        id,
        name: trimmed_name.to_string(),
        description: description.unwrap_or_else(|| "Custom user-added model.".to_string()),
        filename,
        url: trimmed_url,
        size_bytes: size_hint.unwrap_or(0),
        downloaded: false,
        fits: true,
        fit_class: String::new(),
        family: String::new(),
        vision: false,
        builtin: false,
        vision_ready: false,
    };
    list.push(model.clone());
    save_custom_models(&list)?;
    Ok(model)
}

pub fn remove_custom_model(id: &str) -> Result<bool, String> {
    let mut list = load_custom_models();
    let len_before = list.len();
    list.retain(|m| m.id != id);
    let removed = list.len() != len_before;
    if removed {
        save_custom_models(&list)?;
    }
    Ok(removed)
}

/// Get free disk space in bytes for the directory that contains the path.
/// Uses `df -k` for cross-filesystem accuracy on macOS (APFS includes
/// purgeable, which is what users actually have access to).
#[cfg(not(target_os = "windows"))]
fn free_disk_bytes(dir: &std::path::Path) -> Option<u64> {
    let output = std::process::Command::new("df")
        .arg("-k")
        .arg(dir)
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Second line, fourth column = available KB
    let line = stdout.lines().nth(1)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    let avail_kb: u64 = parts.get(3)?.parse().ok()?;
    Some(avail_kb * 1024)
}

/// Windows: GetDiskFreeSpaceExW returns the free bytes the calling process
/// can use on the volume containing `dir` (already accounts for per-user
/// quotas, which is what we actually care about for "do I have room to
/// download a model").
#[cfg(target_os = "windows")]
fn free_disk_bytes(dir: &std::path::Path) -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let abs = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let wide: Vec<u16> = abs.as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let mut free_to_caller: u64 = 0;
    unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide.as_ptr()),
            Some(&mut free_to_caller),
            None,
            None,
        ).ok()?;
    }
    Some(free_to_caller)
}

pub async fn download_model(
    model: &LocalModel,
    progress: &Arc<DownloadProgress>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    

    std::fs::create_dir_all(models_dir())
        .map_err(|e| format!("I couldn't create the models folder. {}", e))?;

    let path = model_path(&model.filename);
    let tmp_path = path.with_extension("gguf.tmp");

    // Weights already here: the only thing left to fetch is the vision
    // projector (models downloaded before every curated model became
    // multimodal have none). Never re-download gigabytes for a 1GB file.
    if path.exists() {
        if is_vision_model(&model.id) && !mmproj_downloaded(&model.id) {
            download_mmproj(&model.id, progress, app).await?;
            crate::event_bus::emit_ui(app, "model-download-progress", serde_json::json!({
                "model_id": model.id,
                "downloaded": 1,
                "total": 1,
                "percent": 100,
            }))
            .ok();
            crate::notifications::os_notify(app, "Orbix Core", &format!("{} can see images now", model.name));
        }
        return Ok(());
    }

    // ── #2: Disk space pre-check ───────────────────────────────
    // Need: model size + 2GB safety buffer. If a partial download
    // already exists, we only need the remainder.
    let already_downloaded = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);
    let need_bytes = model.size_bytes.saturating_sub(already_downloaded) + 2_000_000_000;
    if let Some(free) = free_disk_bytes(&models_dir()) {
        if free < need_bytes {
            let need_gb = need_bytes as f64 / 1_000_000_000.0;
            let free_gb = free as f64 / 1_000_000_000.0;
            return Err(format!(
                "You need about {:.1} GB free to download {}. This device has {:.1} GB available. Free up some space and try again.",
                need_gb, model.name, free_gb
            ));
        }
    }

    progress.is_downloading.store(true, Ordering::SeqCst);
    progress.downloaded_bytes.store(already_downloaded, Ordering::SeqCst);
    progress.total_bytes.store(model.size_bytes, Ordering::SeqCst);
    {
        let mut id = progress.current_model_id.lock().unwrap();
        *id = model.id.clone();
    }

    // ── #1: Resumable download via HTTP Range ─────────────────
    // If a partial .tmp file exists from a previous attempt, send
    // `Range: bytes=N-` and pick up from byte N. Server returns
    // 206 Partial Content with the rest.
    // No TOTAL timeout: multi-GB downloads legitimately run for a long
    // time. (`timeout(from_secs(0))` was a zero-second deadline, not "no
    // timeout" - it killed every download instantly. Regression test:
    // download_client_tests.) Connect timeout stays so a dead network
    // still fails fast.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        // Keep the TCP socket healthy on Windows. The OS default keepalive
        // interval is 2 hours, so a stalled HTTP/2 stream to HF's CDN looks
        // like the download is hanging at 0%. Sending a keepalive every 30s
        // forces the kernel to detect a dead peer in seconds, not hours.
        .tcp_keepalive(std::time::Duration::from_secs(30))
        // Cap idle time in the connection pool so a flaky redirect chain
        // doesn't leave us holding a half-open connection.
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        // The HuggingFace `/resolve/...` endpoint 302's to a signed CDN URL.
        // reqwest follows redirects by default (<=10), but we make it
        // explicit so future code changes don't regress this.
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("HTTP client setup failed: {}", e))?;

    let mut req = client.get(&model.url);
    if already_downloaded > 0 {
        req = req.header("Range", format!("bytes={}-", already_downloaded));
        eprintln!("[download] Resuming {} from byte {}", model.id, already_downloaded);
    }

    let resp = req.send().await
        .map_err(|e| {
            progress.is_downloading.store(false, Ordering::SeqCst);
            format!("I couldn't reach the model server. Check your internet connection and try again. ({})", e)
        })?;

    let status = resp.status();
    let is_resume = already_downloaded > 0 && status.as_u16() == 206;
    let is_fresh = status.as_u16() == 200;

    if !is_resume && !is_fresh {
        progress.is_downloading.store(false, Ordering::SeqCst);
        return Err(format!(
            "The download server returned an error (HTTP {}). The model file may have moved. Try again or pick a different model in Settings.",
            status
        ));
    }

    // Total bytes = what we've got + what the server promises
    let remaining = resp.content_length().unwrap_or(model.size_bytes - already_downloaded);
    let total = if is_resume { already_downloaded + remaining } else { remaining };
    progress.total_bytes.store(total, Ordering::SeqCst);

    // Open the tmp file in append mode if resuming, create otherwise
    let file = if is_resume {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&tmp_path)
            .await
            .map_err(|e| format!("Couldn't reopen the partial download: {}", e))?
    } else {
        // Fresh start - wipe any stale tmp
        let _ = tokio::fs::remove_file(&tmp_path).await;
        tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| format!("Couldn't create the download file: {}", e))?
    };
    let mut writer = tokio::io::BufWriter::new(file);

    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = already_downloaded;
    let mut last_emit: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| {
            // Don't wipe progress on stream error - let the next attempt resume.
            progress.is_downloading.store(false, Ordering::SeqCst);
            format!("Download interrupted ({}). It'll resume from where it left off if you try again.", e)
        })?;
        writer
            .write_all(&chunk)
            .await
            .map_err(|e| format!("Couldn't write to disk: {}", e))?;

        downloaded += chunk.len() as u64;
        progress.downloaded_bytes.store(downloaded, Ordering::SeqCst);

        // Emit progress every ~500KB to avoid flooding
        if downloaded - last_emit > 500_000 {
            crate::event_bus::emit_ui(app, "model-download-progress", serde_json::json!({
                "model_id": model.id,
                "downloaded": downloaded,
                "total": total,
                "percent": ((downloaded as f64 / total as f64) * 100.0) as u32,
                "resumed": is_resume,
            }))
            .ok();
            last_emit = downloaded;
        }
    }

    writer
        .flush()
        .await
        .map_err(|e| format!("Couldn't finalize the download: {}", e))?;

    // Verify before install (W1.3): fail fast on corrupt downloads
    // instead of a 120s server-load timeout later. GGUF files start with
    // the magic bytes "GGUF"; an HTML error page or truncated stream
    // does not.
    {
        use tokio::io::AsyncReadExt;
        let meta = tokio::fs::metadata(&tmp_path)
            .await
            .map_err(|e| format!("Couldn't stat the download: {}", e))?;
        if total > 0 && meta.len() != total {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            progress.is_downloading.store(false, Ordering::SeqCst);
            return Err(format!(
                "Download is incomplete ({} of {} bytes). The file was discarded - try again.",
                meta.len(),
                total
            ));
        }
        let mut f = tokio::fs::File::open(&tmp_path)
            .await
            .map_err(|e| format!("Couldn't reopen the download for verification: {}", e))?;
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic)
            .await
            .map_err(|e| format!("Couldn't read the download for verification: {}", e))?;
        if &magic != b"GGUF" {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            progress.is_downloading.store(false, Ordering::SeqCst);
            return Err("Downloaded file is not a GGUF model (bad magic bytes). \
                 The URL may point to an error page - the file was discarded."
                .to_string());
        }
    }

    // Rename tmp to final
    tokio::fs::rename(&tmp_path, &path)
        .await
        .map_err(|e| format!("Couldn't move the file into place: {}", e))?;

    // Vision models need their projector (mmproj) too, or images silently
    // drop. Fetch it now, on the same progress bar, so "download the model"
    // means the model can actually see. A projector failure is soft: the
    // weights are already saved and usable for text, so we warn instead of
    // discarding the whole download.
    if is_vision_model(&model.id) && !mmproj_downloaded(&model.id) {
        if let Err(e) = download_mmproj(&model.id, progress, app).await {
            eprintln!("[download] projector for {} failed: {}", model.id, e);
            crate::event_bus::emit_ui(app, "model-download-warning", serde_json::json!({
                "model_id": model.id,
                "message": format!("{} downloaded, but its vision projector didn't. Text works; re-download to enable images.", model.name),
            })).ok();
        }
    }

    progress.is_downloading.store(false, Ordering::SeqCst);

    crate::event_bus::emit_ui(app, "model-download-progress", serde_json::json!({
        "model_id": model.id,
        "downloaded": total,
        "total": total,
        "percent": 100,
    }))
    .ok();

    // Native completion notification via the plugin so it carries the
    // Orbix Core icon on both platforms (Script Editor's on macOS and a
    // bare label on Windows otherwise).
    crate::notifications::os_notify(
        app,
        "Orbix Core",
        &format!("{} is ready to use", model.name),
    );

    Ok(())
}

/// Fetch a vision model's projector (mmproj) into the models folder if it
/// isn't already there. Kept separate from `download_model`'s main-weights
/// path (which is timing-sensitive and hard-won) but mirrors its safeguards:
/// disk pre-check, resumable Range download, GGUF magic verify, atomic
/// rename. Reuses the same progress atomics so the UI bar reflects this
/// phase. No-op for text models.
pub async fn download_mmproj(
    model_id: &str,
    progress: &Arc<DownloadProgress>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let Some(mm) = mmproj_for(model_id) else { return Ok(()); };
    let path = model_path(&mm.filename);
    if path.exists() { return Ok(()); }
    let tmp_path = path.with_extension("gguf.tmp");
    let already = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);

    let need = mm.size_bytes.saturating_sub(already) + 500_000_000;
    if let Some(free) = free_disk_bytes(&models_dir()) {
        if free < need {
            return Err(format!(
                "Not enough space for {}'s vision projector (need about {:.1} GB).",
                model_id, need as f64 / 1_000_000_000.0
            ));
        }
    }

    progress.is_downloading.store(true, Ordering::SeqCst);
    progress.downloaded_bytes.store(already, Ordering::SeqCst);
    progress.total_bytes.store(mm.size_bytes, Ordering::SeqCst);
    {
        let mut id = progress.current_model_id.lock().unwrap_or_else(|e| e.into_inner());
        *id = model_id.to_string();
    }

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        // Keep the TCP socket healthy on Windows. The OS default keepalive
        // interval is 2 hours, so a stalled HTTP/2 stream to HF's CDN looks
        // like the download is hanging at 0%. Sending a keepalive every 30s
        // forces the kernel to detect a dead peer in seconds, not hours.
        .tcp_keepalive(std::time::Duration::from_secs(30))
        // Cap idle time in the connection pool so a flaky redirect chain
        // doesn't leave us holding a half-open connection.
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        // The HuggingFace `/resolve/...` endpoint 302's to a signed CDN URL.
        // reqwest follows redirects by default (<=10), but we make it
        // explicit so future code changes don't regress this.
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("HTTP client setup failed: {}", e))?;
    let mut req = client.get(&mm.url);
    if already > 0 {
        req = req.header("Range", format!("bytes={}-", already));
    }
    let resp = req.send().await
        .map_err(|e| format!("Couldn't reach the projector download: {}", e))?;
    let status = resp.status();
    let is_resume = already > 0 && status.as_u16() == 206;
    let is_fresh = status.as_u16() == 200;
    if !is_resume && !is_fresh {
        return Err(format!("The projector download returned HTTP {}.", status));
    }
    let remaining = resp.content_length().unwrap_or(mm.size_bytes.saturating_sub(already));
    let total = if is_resume { already + remaining } else { remaining };
    progress.total_bytes.store(total, Ordering::SeqCst);

    let file = if is_resume {
        tokio::fs::OpenOptions::new().append(true).open(&tmp_path).await
            .map_err(|e| format!("Couldn't reopen the partial projector: {}", e))?
    } else {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        tokio::fs::File::create(&tmp_path).await
            .map_err(|e| format!("Couldn't create the projector file: {}", e))?
    };
    let mut writer = tokio::io::BufWriter::new(file);
    let mut stream = resp.bytes_stream();
    let mut downloaded = already;
    let mut last_emit = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|e| format!("Projector download interrupted ({}). It'll resume next time.", e))?;
        writer.write_all(&chunk).await
            .map_err(|e| format!("Couldn't write the projector to disk: {}", e))?;
        downloaded += chunk.len() as u64;
        progress.downloaded_bytes.store(downloaded, Ordering::SeqCst);
        if downloaded - last_emit > 500_000 {
            crate::event_bus::emit_ui(app, "model-download-progress", serde_json::json!({
                "model_id": model_id,
                "downloaded": downloaded,
                "total": total,
                "percent": if total > 0 { ((downloaded as f64 / total as f64) * 100.0) as u32 } else { 0 },
                "phase": "projector",
            })).ok();
            last_emit = downloaded;
        }
    }
    writer.flush().await
        .map_err(|e| format!("Couldn't finalize the projector: {}", e))?;

    {
        use tokio::io::AsyncReadExt;
        let mut f = tokio::fs::File::open(&tmp_path).await
            .map_err(|e| format!("Couldn't reopen the projector for verification: {}", e))?;
        let mut magic = [0u8; 4];
        f.read_exact(&mut magic).await
            .map_err(|e| format!("Couldn't read the projector for verification: {}", e))?;
        if &magic != b"GGUF" {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err("The projector download isn't a valid GGUF (bad magic bytes). Discarded.".to_string());
        }
    }
    tokio::fs::rename(&tmp_path, &path).await
        .map_err(|e| format!("Couldn't move the projector into place: {}", e))?;
    Ok(())
}

pub async fn delete_model(filename: &str) -> Result<(), String> {
    let path = model_path(filename);
    if path.exists() {
        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| format!("Delete failed: {}", e))?;
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  llama-server process management
// ─────────────────────────────────────────────

/// Launch the next server with its vision projector. Set by
/// bob_core::ensure_model_loaded_for for a turn with images; every other
/// launch is text-only so the prompt cache can be saved and restored.
pub static LAUNCH_VISION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the running server was launched with the prompt cache on.
pub static PROMPT_CACHE_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn prompt_cache_dir() -> std::path::PathBuf {
    crate::storage::base_dir().join("prompt_cache")
}

/// The cache file for the running server: model, its file size (a
/// re-downloaded different quant never gets another file's cache), per-slot
/// context and slot count. With the server's slot ids.
async fn prompt_cache_key(endpoint: &str, model_id: &str) -> Option<(String, Vec<serde_json::Value>)> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(3)).build().ok()?;
    let slots: Vec<serde_json::Value> = client.get(format!("{}/slots", endpoint)).send().await.ok()?.json().await.ok()?;
    let n_ctx = slots.first()?.get("n_ctx")?.as_u64()?;
    let size = find_model(model_id)
        .and_then(|m| std::fs::metadata(model_path(&m.filename)).ok())
        .map(|m| m.len())
        .unwrap_or(0);
    Some((format!("{}-{}-{}x{}.bin", model_id, size, n_ctx, slots.len()), slots))
}

/// Save the slot holding the most prompt (Bob's prefix) before an unload.
pub async fn save_prompt_cache(endpoint: &str, model_id: &str) {
    if !PROMPT_CACHE_ON.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some((file, slots)) = prompt_cache_key(endpoint, model_id).await else { return };
    let warmest = slots
        .iter()
        .filter(|s| s["n_prompt_tokens"].as_u64().unwrap_or(0) > 0)
        .max_by_key(|s| s["n_prompt_tokens"].as_u64().unwrap_or(0));
    let Some(id) = warmest.and_then(|s| s["id"].as_u64()) else { return };
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() else { return };
    let res = client
        .post(format!("{}/slots/{}?action=save", endpoint, id))
        .json(&serde_json::json!({ "filename": file }))
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {
            eprintln!("[prompt_cache] saved {}", file);
            // One file per model (~170 MB on E4B): drop this model's older
            // ones (another context size or a replaced download).
            if let Ok(entries) = std::fs::read_dir(prompt_cache_dir()) {
                for e in entries.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with(&format!("{}-", model_id)) && name != file {
                        let _ = std::fs::remove_file(e.path());
                    }
                }
            }
        }
        Ok(r) => eprintln!("[prompt_cache] save refused: {}", r.status()),
        Err(e) => eprintln!("[prompt_cache] save failed: {}", e),
    }
}

/// Restore the saved prompt into every slot after a load, so whichever
/// slot takes the next turn already holds Bob's prefix.
pub async fn restore_prompt_cache(endpoint: &str, model_id: &str) {
    if !PROMPT_CACHE_ON.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let Some((file, slots)) = prompt_cache_key(endpoint, model_id).await else { return };
    if !prompt_cache_dir().join(&file).exists() {
        return;
    }
    let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() else { return };
    for id in slots.iter().filter_map(|s| s["id"].as_u64()) {
        let _ = client
            .post(format!("{}/slots/{}?action=restore", endpoint, id))
            .json(&serde_json::json!({ "filename": file }))
            .send()
            .await;
    }
    eprintln!("[prompt_cache] restored {}", file);
}

pub struct LlamaServerProcess {
    pub child: Option<tokio::process::Child>,
    pub model_id: String,
    /// Launched with the vision projector (a turn carried an image).
    pub vision: bool,
    pub port: u16,
    /// When the model was loaded (for uptime display).
    pub loaded_at: Option<std::time::Instant>,
    /// Last time a request was served - used for idle eviction.
    pub last_used: Option<std::time::Instant>,
}

impl Default for LlamaServerProcess {
    fn default() -> Self {
        Self {
            child: None,
            model_id: String::new(),
            vision: false,
            port: llama_port(),
            loaded_at: None,
            last_used: None,
        }
    }
}

/// Info about a loaded model - for the UI model pool indicator.
#[derive(Clone, Debug, serde::Serialize)]
pub struct LoadedModelInfo {
    pub model_id: String,
    pub port: u16,
    pub loaded_secs: u64,
    pub idle_secs: u64,
}

pub async fn start_server(
    model_filename: &str,
    _model_id: &str,
) -> Result<tokio::process::Child, String> {
    start_server_on_port(model_filename, _model_id, llama_port()).await
}

pub async fn start_agent_server(
    model_filename: &str,
    model_id: &str,
) -> Result<tokio::process::Child, String> {
    start_server_on_port(model_filename, model_id, agent_port()).await
}

/// Start the server, and on Windows recover from a broken GPU build: if
/// the cuda/vulkan variant fails to come up (bad driver, missing DLL,
/// OOM at load), pin the CPU build via the fallback marker and retry
/// once, so the user gets a working - if slower - model instead of an
/// error. Port conflicts are excluded: those fail identically on any
/// variant and the real fix is freeing the port.
async fn start_server_on_port(
    model_filename: &str,
    model_id: &str,
    port: u16,
) -> Result<tokio::process::Child, String> {
    let first = start_server_attempt(model_filename, model_id, port).await;
    #[cfg(target_os = "windows")]
    {
        match first {
            Ok(child) => Ok(child),
            Err(e) => {
                let variant = llama_variant();
                let port_conflict = e.contains("already in use") || e.contains("could not bind");
                if variant == "cpu" || port_conflict {
                    return Err(e);
                }
                log::warn!(
                    "[local_llm] {} build failed to start ({}); falling back to the CPU build",
                    variant, e
                );
                let _ = std::fs::write(
                    variant_fallback_path(),
                    format!(
                        "The {} llama-server build failed to start; Orbix pinned the CPU build.\n\
                         Delete this file (or set ORBIX_LLAMA_VARIANT) to retry the GPU build.\n\
                         Original error:\n{}\n",
                        variant, e
                    ),
                );
                start_server_attempt(model_filename, model_id, port)
                    .await
                    .map_err(|e2| format!("{} build failed ({}); CPU fallback also failed: {}", variant, e, e2))
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    first
}

async fn start_server_attempt(
    model_filename: &str,
    model_id: &str,
    port: u16,
) -> Result<tokio::process::Child, String> {
    let server_path = ensure_llama_server().await?;
    let model_file = model_path(model_filename);

    if !model_file.exists() {
        return Err(format!("Model file not found: {}", model_filename));
    }

    // "Large" is a size, not a name: anything over 10GB of weights gets
    // the single-slot + quantized-KV treatment on sub-32GB devices. (The
    // old filename test missed the 22GB Qwen3.6-35B file entirely.)
    let is_large_model = std::fs::metadata(&model_file)
        .map(|m| m.len() > 10 * 1_073_741_824)
        .unwrap_or(false);

    // Check available system memory to scale context window
    let total_mem_gb = total_memory_gb() as u64;

    // Context window - large models use smaller windows to keep KV cache modest.
    // Note: KV cache size = ctx * num_layers * hidden_dim * 2 (K+V) * dtype_bytes.
    // Quantizing KV cache (cache-type-k/v = q4_0) cuts dtype_bytes from 2 → 0.5.
    // Long HTML/code answers were dying against the context wall, not
    // max_tokens - generous tiers beat a hard cap (field feedback
    // 2026-08-01). KV stays affordable via the quantized cache.
    // Context from HEADROOM (usable memory minus the model itself), not
    // raw RAM tiers: a 4B model on a 24GB machine deserves 32k ctx, a
    // 26B on the same machine does not. Fixed overhead per turn (system
    // prompt + tool schemas) is ~8-9k tokens, so small windows starve
    // the conversation.
    let weights_gb = std::fs::metadata(&model_file)
        .map(|m| m.len() as f64 / 1_073_741_824.0)
        .unwrap_or(8.0);
    let headroom_gb = (total_mem_gb as f64 * 0.70) - (weights_gb * 1.15);
    let ctx_size = if headroom_gb >= 8.0 { "32768" }
        else if headroom_gb >= 4.0 { "16384" }
        else { "8192" };
    CURRENT_CTX_SIZE.store(ctx_size.parse().unwrap_or(8192), std::sync::atomic::Ordering::SeqCst);

    // GPU offload: full offload on macOS (llama.cpp handles unified memory
    // on Apple Silicon) and on Windows when a GPU build (cuda/vulkan) is
    // selected; the CPU build ignores the flag but 0 keeps intent clear.
    #[cfg(target_os = "macos")]
    let gpu_layers = "99";
    #[cfg(target_os = "windows")]
    let gpu_layers = if llama_variant() == "cpu" { "0" } else { "99" };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let gpu_layers = "0";

    // Single slot for large models on tight RAM - drops KV cache to one buffer
    let parallel = if is_large_model && total_mem_gb < 32 { "1" } else { "2" };

    // Free the port before spawning. A prior app session can exit
    // without reaping its llama-server, leaving an orphan (reparented to
    // launchd) still bound to this port; the fresh spawn would then fail
    // with "couldn't bind socket" and exit 1. Reclaim our own stale
    // server; refuse to touch a foreign process.
    reclaim_port(port).await?;

    // Refuse models that cannot fit - loading succeeds and then every
    // request dies with a Metal OOM, which surfaces to the user as an
    // empty response. An honest refusal with numbers beats that.
    if let Ok(meta) = std::fs::metadata(&model_file) {
        let projector = mmproj_for(model_id)
            .map(|mm| std::fs::metadata(model_path(&mm.filename)).map(|m| m.len()).unwrap_or(0))
            .unwrap_or(0);
        if !fits_in_memory(meta.len() + projector) {
            let model_gb = (meta.len() + projector) as f64 / 1_073_741_824.0;
            let total = total_memory_gb();
            return Err(format!(
                "{} needs about {:.0} GB of memory to run, but this device has {:.0} GB total (about {:.0} GB usable for AI). Pick a model under ~{:.0} GB in Settings - Models.",
                model_id, model_gb * 1.15, total, total * 0.70, total * 0.60
            ));
        }
    }

    let server_dir = server_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    // Use the no-window helper so llama-server.exe - a Windows
    // console-subsystem binary - doesn't flash a console window every time
    // we (re)start it. On macOS it's a plain Command.
    let mut cmd = crate::process_ext::tokio_command(&server_path);

    // Library discovery:
    //   macOS: DYLD_LIBRARY_PATH so the dynamic linker finds the .dylibs
    //          colocated with llama-server.
    //   Windows: the DLL loader checks the directory of the executable
    //          first, which is already where the DLLs live - no env needed.
    // Either way, run from the server's own folder so relative paths resolve.
    #[cfg(target_os = "macos")]
    cmd.env("DYLD_LIBRARY_PATH", server_dir);
    cmd.current_dir(server_dir);

    cmd.arg("--model")
        .arg(&model_file)
        .arg("--port")
        .arg(port.to_string())
        .arg("--ctx-size")
        .arg(ctx_size)
        .arg("--n-gpu-layers")
        .arg(gpu_layers)
        .arg("--parallel")
        .arg(parallel)
        .arg("--no-warmup")
        // Flash attention: smaller KV cache, faster inference, no quality loss.
        // Since llama.cpp b8753 this flag REQUIRES a value: on|off|auto (still true in b9969).
        // Adding it as a bare flag crashed the server on startup.
        .arg("--flash-attn").arg("on")
        // Reuse cached KV chunks after the point where a prompt diverges
        // (the per-turn context block moves from one user message to the
        // next), so a long conversation does not re-read its whole
        // history every turn - only the system+tools prefix used to hit.
        .arg("--cache-reuse").arg("256");

    // Text-only by default, with the prompt cache: Bob's fixed prompt
    // (~9.2k tokens) is saved to disk before an idle unload and restored
    // after the reload, so the first turn after a break reads it in ~1s
    // instead of ~18s (measured 2026-10-21, Gemma 4 E4B). llama-server
    // only saves slots without a projector, and Gemma 4's sliding-window
    // layers are reusable after a restore only with --swa-full (+1.2 GB
    // while loaded on E4B; skipped where RAM is already tight).
    // Consumed: only the launch it was set for gets the projector.
    let vision = LAUNCH_VISION.swap(false, std::sync::atomic::Ordering::SeqCst);
    let prompt_cache = !vision && !(is_large_model && total_mem_gb < 32);
    PROMPT_CACHE_ON.store(prompt_cache, std::sync::atomic::Ordering::SeqCst);
    if prompt_cache {
        let dir = prompt_cache_dir();
        let _ = std::fs::create_dir_all(&dir);
        cmd.arg("--swa-full").arg("--slot-save-path").arg(&dir);
    }

    // Vision: only when a turn carries an image (LAUNCH_VISION). The
    // projector lets llama-server accept image_url content parts; without
    // --mmproj it runs text-only and ignores images. If the projector isn't
    // on disk (e.g. its download failed), start text-only and let the
    // vision status surface it rather than refusing to launch.
    if let Some(mm) = mmproj_for(model_id).filter(|_| vision) {
        let mmproj_path = model_path(&mm.filename);
        if mmproj_path.exists() {
            cmd.arg("--mmproj").arg(&mmproj_path);
        } else {
            eprintln!("[local_llm] {} is a vision model but its projector is missing; images disabled", model_id);
        }
    }

    // For large models on tight RAM, quantize the KV cache (4x smaller).
    // q4_0 is the most-tested KV quant - minimal quality impact, huge memory win.
    // KV cache for 26B at 4096 ctx: ~3GB → ~750MB.
    if is_large_model && total_mem_gb < 32 {
        cmd.arg("--cache-type-k").arg("q4_0")
           .arg("--cache-type-v").arg("q4_0");
    }

    // Capture stderr into a bounded ring buffer instead of discarding it,
    // so a startup failure reports the server's ACTUAL last words rather
    // than a guess. A reader task also drains the pipe so its buffer
    // never fills and blocks the server mid-log.
    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start llama-server: {}", e))?;

    let stderr_tail: Arc<std::sync::Mutex<std::collections::VecDeque<String>>> =
        Arc::new(std::sync::Mutex::new(std::collections::VecDeque::with_capacity(STDERR_TAIL_LINES)));
    if let Some(stderr) = child.stderr.take() {
        let tail = Arc::clone(&stderr_tail);
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut buf = tail.lock().unwrap_or_else(|e| e.into_inner());
                if buf.len() == STDERR_TAIL_LINES {
                    buf.pop_front();
                }
                buf.push_back(line);
            }
        });
    }

    // Wait for server to be ready
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", port);

    // Large models can take 60+ seconds to load; poll for up to 120s,
    // but fail FAST if the process dies (e.g. a dyld error) instead of
    // pinging a corpse for the whole window.
    for _ in 0..240 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if let Ok(Some(status)) = child.try_wait() {
            let tail = stderr_tail.lock().unwrap_or_else(|e| e.into_inner()).clone();
            return Err(diagnose_exit(status, &tail, port));
        }
        if let Ok(resp) = client.get(&health_url).send().await {
            if resp.status().is_success() {
                return Ok(child);
            }
        }
    }

    Err("llama-server failed to start within 120s".to_string())
}

/// How many trailing stderr lines to keep for diagnostics.
const STDERR_TAIL_LINES: usize = 40;

/// Turn a llama-server early exit into a message keyed on what actually
/// went wrong, using its captured stderr - never a blanket "the binary
/// is broken" guess. Only a genuine load/link failure should ever send
/// the user to re-download.
fn diagnose_exit(
    status: std::process::ExitStatus,
    stderr_tail: &std::collections::VecDeque<String>,
    port: u16,
) -> String {
    let joined = stderr_tail
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let low = joined.to_lowercase();

    if low.contains("bind") || low.contains("address already in use") || low.contains("http server error") {
        return format!(
            "Port {port} was taken when llama-server tried to start, so it could not bind. \
             Another Orbix instance or a leftover model server is using it. Relaunch (Orbix now reclaims a stale server automatically), or set ORBIX_LLAMA_PORT to a free port."
        );
    }
    // Windows: a missing DLL kills the process before main() with
    // STATUS_DLL_NOT_FOUND and nothing on stderr.
    #[cfg(target_os = "windows")]
    if status.code() == Some(-1073741515) {
        return format!(
            "llama-server could not load its DLLs (STATUS_DLL_NOT_FOUND). This is a broken or \
             mismatched binary: quit the app, delete the bin folder inside %LOCALAPPDATA%\\{}, \
             and relaunch to re-download.",
            DATA_DIR_NAME
        );
    }
    if low.contains("dyld")
        || low.contains("library not loaded")
        || low.contains("image not found")
        || low.contains("symbol not found")
        || low.contains("code signature")
        || low.contains("specified module could not be found")
    {
        return format!(
            "llama-server could not load its libraries ({}). This is a broken or mismatched binary: \
             quit the app, delete the bin folder inside Application Support/{}, and relaunch to re-download.\n{}",
            status, DATA_DIR_NAME, tail_snippet(&joined)
        );
    }
    if low.contains("failed to load model")
        || low.contains("error loading model")
        || low.contains("unable to load model")
    {
        return format!(
            "llama-server could not load the model file (it may be corrupt or truncated). \
             Re-download the model in Settings > Models.\n{}",
            tail_snippet(&joined)
        );
    }
    if low.contains("out of memory") || low.contains("failed to allocate") || low.contains("insufficient memory") {
        return format!(
            "llama-server ran out of memory loading this model. Pick a smaller model in Settings > Models.\n{}",
            tail_snippet(&joined)
        );
    }
    // Unknown cause: surface the real stderr instead of guessing.
    if joined.trim().is_empty() {
        format!("llama-server exited during startup ({status}) with no output.")
    } else {
        format!(
            "llama-server exited during startup ({status}). Last output:\n{}",
            tail_snippet(&joined)
        )
    }
}

/// The last handful of stderr lines, for embedding in an error.
fn tail_snippet(joined: &str) -> String {
    joined
        .lines()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Who is listening on a TCP port right now: (pid, full command line).
/// Returns None if nothing holds the port (or the tools are absent).
#[cfg(not(target_os = "windows"))]
fn port_listener(port: u16) -> Option<(i32, String)> {
    // lsof gives us the PID cheaply; its own command field is the short
    // truncated name, so we read the FULL command line from ps - the
    // reclaim guard needs the path to tell our binary from a foreign one.
    let out = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{}", port), "-sTCP:LISTEN", "-t"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let pid: i32 = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()?
        .trim()
        .parse()
        .ok()?;
    let cmd = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Some((pid, cmd))
}

/// Windows: netstat -ano gives the owning PID of a listening socket with
/// no elevation; the full command line comes from CIM so the reclaim
/// guard can tell our binary from a foreign one.
#[cfg(target_os = "windows")]
fn port_listener(port: u16) -> Option<(i32, String)> {
    let out = crate::process_ext::std_command("netstat.exe")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let needle = format!(":{}", port);
    let pid: i32 = text
        .lines()
        .filter(|l| l.to_uppercase().contains("LISTENING"))
        .find(|l| {
            l.split_whitespace()
                .nth(1)
                .map(|a| a.ends_with(&needle))
                .unwrap_or(false)
        })
        .and_then(|l| l.split_whitespace().last())
        .and_then(|p| p.parse().ok())?;
    let cmd = crate::process_ext::std_command("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "(Get-CimInstance Win32_Process -Filter \"ProcessId={}\").CommandLine",
                pid
            ),
        ])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Some((pid, cmd))
}

/// True when a process command belongs to one of our own llama-server
/// binaries (matched by the data-dir path), so reclaiming it is safe.
fn is_our_llama_server(cmd: &str) -> bool {
    (cmd.contains("llama-server") || cmd.contains("llama_server"))
        && cmd.contains(DATA_DIR_NAME)
}

/// Make `port` bindable before we spawn. If one of our own llama-servers
/// (typically an orphan reparented to launchd after a prior app session
/// exited without cleanup) is holding it, kill it and wait for release.
/// If an unrelated process holds it, refuse to kill it and say so.
/// Returns Ok(()) when the port is free to bind.
#[cfg(not(target_os = "windows"))]
async fn reclaim_port(port: u16) -> Result<(), String> {
    let Some((pid, cmd)) = port_listener(port) else {
        return Ok(()); // nothing there
    };
    if !is_our_llama_server(&cmd) {
        return Err(format!(
            "Port {port} is already in use by another program (PID {pid}: {}). \
             Quit that program, or set ORBIX_LLAMA_PORT to a free port and relaunch.",
            cmd
        ));
    }
    log::warn!("[local_llm] reclaiming port {port} from stale llama-server PID {pid}");
    // SIGTERM, then escalate to SIGKILL, polling until the socket frees.
    let signal = |sig: &str, pid: i32| {
        let _ = std::process::Command::new("kill")
            .arg(sig)
            .arg(pid.to_string())
            .output();
    };
    signal("-TERM", pid);
    for i in 0..40 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if port_listener(port).is_none() {
            return Ok(());
        }
        if i == 10 {
            signal("-KILL", pid);
        }
    }
    Err(format!(
        "Port {port} is held by a stale llama-server (PID {pid}) that would not exit. \
         Quit it in Activity Monitor and relaunch."
    ))
}

/// Windows reclaim: taskkill sends a close request first, then escalates
/// to a forced kill if the stale server ignores it.
#[cfg(target_os = "windows")]
async fn reclaim_port(port: u16) -> Result<(), String> {
    let Some((pid, cmd)) = port_listener(port) else {
        return Ok(()); // nothing there
    };
    if !is_our_llama_server(&cmd) {
        return Err(format!(
            "Port {port} is already in use by another program (PID {pid}: {}). \
             Quit that program, or set ORBIX_LLAMA_PORT to a free port and relaunch.",
            cmd
        ));
    }
    log::warn!("[local_llm] reclaiming port {port} from stale llama-server PID {pid}");
    let kill = |force: bool, pid: i32| {
        let mut c = crate::process_ext::std_command("taskkill.exe");
        c.args(["/PID", &pid.to_string()]);
        if force {
            c.arg("/F");
        }
        let _ = c.output();
    };
    kill(false, pid);
    for i in 0..40 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if port_listener(port).is_none() {
            return Ok(());
        }
        if i == 10 {
            kill(true, pid);
        }
    }
    Err(format!(
        "Port {port} is held by a stale llama-server (PID {pid}) that would not exit. \
         End it in Task Manager and relaunch."
    ))
}

pub fn server_endpoint() -> String {
    format!("http://127.0.0.1:{}", llama_port())
}

/// Kill only *our own* llama-servers on our two ports, matched by the data-dir
/// path in their command line. Used by the app's quit paths in place of a
/// blanket `pkill -f llama-server`, which would also cut down LM Studio, a
/// user's own llama.cpp, or any other app's server. Synchronous and quick so
/// it is safe to call from an exit handler.
pub fn kill_our_llama_servers() {
    for port in [llama_port(), agent_port()] {
        if let Some((pid, cmd)) = port_listener(port) {
            if is_our_llama_server(&cmd) {
                #[cfg(windows)]
                {
                    let _ = crate::process_ext::std_command("taskkill.exe")
                        .args(["/PID", &pid.to_string(), "/F"])
                        .output();
                }
                #[cfg(not(windows))]
                {
                    let _ = std::process::Command::new("kill")
                        .arg("-TERM")
                        .arg(pid.to_string())
                        .output();
                }
            }
        }
    }
}

pub fn agent_server_endpoint() -> String {
    format!("http://127.0.0.1:{}", agent_port())
}

/// Check if the agent server (E4B) is running
pub async fn is_agent_server_running() -> bool {
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", agent_port());
    if let Ok(resp) = client.get(&health_url).send().await {
        resp.status().is_success()
    } else {
        false
    }
}

pub async fn stop_server(child: &mut Option<tokio::process::Child>) {
    if let Some(ref mut c) = child {
        c.kill().await.ok();
        c.wait().await.ok();
    }
    *child = None;
}

pub async fn is_server_running() -> bool {
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/health", llama_port());
    if let Ok(resp) = client.get(&health_url).send().await {
        resp.status().is_success()
    } else {
        false
    }
}


// ─────────────────────────────────────────────
//  Tests - Windows build-variant selection
// ─────────────────────────────────────────────

#[cfg(all(test, target_os = "windows"))]
mod variant_tests {
    use super::*;

    // Whatever this machine's drivers are, the resolved variant must be
    // one we have a verified release asset for.
    #[test]
    fn resolved_variant_is_always_supported() {
        let v = llama_variant();
        assert!(["cpu", "cuda", "vulkan"].contains(&v.as_str()), "{v}");
    }

    #[test]
    fn windows_download_url_is_a_known_asset_shape() {
        let url = llama_download_url().unwrap();
        let v = llama_server_version();
        assert!(
            url.contains(&format!("/download/{v}/llama-{v}-bin-win-")),
            "unexpected asset URL: {url}"
        );
        assert!(url.ends_with("-x64.zip"), "{url}");
    }

    #[test]
    fn cudart_companion_lives_in_the_same_release() {
        let url = cudart_download_url();
        assert!(url.contains(&format!("/download/{}/", llama_server_version())), "{url}");
        assert!(url.contains("cudart-llama-bin-win-cuda-"), "{url}");
    }

    #[test]
    fn server_path_is_keyed_on_version_and_variant() {
        let p = llama_server_path();
        let s = p.to_string_lossy().to_string();
        assert!(s.ends_with("llama-server.exe"), "{s}");
        assert!(
            s.contains(&format!("llama-{}-{}", llama_server_version(), llama_variant())),
            "{s}"
        );
    }
}

// ─────────────────────────────────────────────
//  Tests - startup diagnostics (llama-server bring-up)
// ─────────────────────────────────────────────

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::collections::VecDeque;

    fn tail(lines: &[&str]) -> VecDeque<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    // A real ExitStatus with code 1, via the platform shell.
    fn exit1() -> std::process::ExitStatus {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd").args(["/C", "exit 1"]).status().unwrap()
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("sh").args(["-c", "exit 1"]).status().unwrap()
        }
    }

    #[test]
    fn port_conflict_message_names_the_port_not_the_binary() {
        // The exact real-world failure: this must NOT tell the user to
        // delete and re-download a working binary.
        let t = tail(&[
            "E srv start: couldn't bind HTTP server socket, hostname: 127.0.0.1, port: 8847",
            "E srv llama_server: exiting due to HTTP server error",
        ]);
        let msg = diagnose_exit(exit1(), &t, 8847);
        assert!(msg.contains("8847"), "should name the port: {msg}");
        assert!(msg.to_lowercase().contains("bind") || msg.contains("taken"));
        assert!(!msg.to_lowercase().contains("delete the bin"), "must not send them to re-download: {msg}");
    }

    #[test]
    fn dyld_failure_is_the_only_re_download_case() {
        let t = tail(&["dyld: Library not loaded: @rpath/libllama.dylib", "Reason: image not found"]);
        let msg = diagnose_exit(exit1(), &t, 8847);
        assert!(msg.to_lowercase().contains("delete the bin"), "dyld error should offer re-download: {msg}");
    }

    #[test]
    fn model_and_oom_failures_are_distinguished() {
        let model = diagnose_exit(exit1(), &tail(&["error loading model: bad magic"]), 8847);
        assert!(model.to_lowercase().contains("model"), "{model}");
        assert!(!model.to_lowercase().contains("delete the bin"));

        let oom = diagnose_exit(exit1(), &tail(&["ggml_metal: failed to allocate buffer"]), 8847);
        assert!(oom.to_lowercase().contains("memory") || oom.to_lowercase().contains("smaller model"), "{oom}");
    }

    #[test]
    fn unknown_failure_surfaces_real_stderr_not_a_guess() {
        let t = tail(&["some novel panic we have never seen"]);
        let msg = diagnose_exit(exit1(), &t, 8847);
        assert!(msg.contains("some novel panic"), "must echo the real output: {msg}");
        assert!(!msg.to_lowercase().contains("may be broken"));
    }

    #[test]
    fn empty_stderr_says_no_output_rather_than_inventing_a_cause() {
        let msg = diagnose_exit(exit1(), &VecDeque::new(), 8847);
        assert!(msg.to_lowercase().contains("no output"), "{msg}");
    }

    #[test]
    fn only_our_own_binaries_are_reclaimable() {
        assert!(is_our_llama_server(
            "/Users/x/Library/Application Support/ai.orbixcore.app/bin/llama-b9969/llama-server --port 8847"
        ));
        // A foreign process on the same port must never be killed.
        assert!(!is_our_llama_server("/opt/homebrew/bin/some-other-server --port 8847"));
        assert!(!is_our_llama_server("/usr/local/bin/llama-server")); // llama, but not ours
    }
}

// ─────────────────────────────────────────────
//  Tests - vision registry (A2)
// ─────────────────────────────────────────────

#[cfg(test)]
mod vision_tests {
    use super::*;

    #[test]
    fn every_curated_chat_model_has_a_projector() {
        for m in chat_models() {
            let id = m.id.as_str();
            assert!(is_vision_model(id), "{id} should be vision-capable");
            let mm = mmproj_for(id).expect("projector");
            assert!(mm.url.starts_with("https://huggingface.co/"), "{id} url: {}", mm.url);
            assert!(mm.filename.ends_with(".gguf"), "{id} filename: {}", mm.filename);
            assert!(mm.size_bytes > 100_000_000, "{id} size looks wrong: {}", mm.size_bytes);
            // The projector comes from the same repo as the weights.
            let repo = m.url.split("/resolve/").next().unwrap();
            assert!(mm.url.starts_with(repo), "{id}: projector repo differs from weights repo");
        }
    }

    #[test]
    fn embedding_and_unknown_models_are_not_vision() {
        for id in ["qwen3-embedding-0.6b", "gpt-oss-20b", "mistral-7b-v03", "does-not-exist"] {
            assert!(!is_vision_model(id), "{id} must not be vision");
            assert!(mmproj_for(id).is_none());
        }
    }

    /// The catalog is deliberately Gemma + Qwen only - a regression here
    /// means someone re-added a third model family.
    #[test]
    fn projector_filenames_are_unique() {
        // Every upstream repo ships `mmproj-F16.gguf`; local names must
        // not collide in the shared models folder.
        let names: Vec<String> = chat_models()
            .iter()
            .map(|m| mmproj_for(&m.id).unwrap().filename)
            .collect();
        let mut deduped = names.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(names.len(), deduped.len(), "projector filenames collide: {names:?}");
    }

    #[test]
    fn catalog_is_gemma_and_qwen_only() {
        for m in available_models() {
            assert!(matches!(m.family.as_str(), "gemma" | "qwen"), "{} has family {:?}", m.id, m.family);
            assert!(m.builtin);
            assert!(m.url.starts_with("https://huggingface.co/"));
            assert!(m.size_bytes > 0);
        }
        let ids: Vec<String> = available_models().iter().map(|m| m.id.clone()).collect();
        let mut deduped = ids.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(ids.len(), deduped.len(), "duplicate ids: {ids:?}");
        // Ids that were retired must not come back by accident.
        for gone in ["gpt-oss-20b", "mistral-7b-v03", "llama-3.2-3b", "phi-3.5-mini", "qwen-2.5-7b", "qwen-2.5-3b", "qwen-2.5-vl-32b", "pixtral-12b", "minicpm-v-2.6", "bge-small-en", "nomic-embed-text"] {
            assert!(!ids.iter().any(|i| i == gone), "{gone} is back in the catalog");
        }
    }

    #[test]
    fn recommendation_walks_best_first() {
        // The first curated chat model that fits comfortably is the pick;
        // on any device at least the E2B fits, so there is always one.
        let rec = recommended_model_id().expect("a recommendation");
        assert!(chat_models().iter().any(|m| m.id == rec));
        assert!(!is_embedding_model(&rec));
    }
}

// ─────────────────────────────────────────────
//  Tests - model resolution (W1.1)
// ─────────────────────────────────────────────

#[cfg(test)]
mod resolution_tests {
    use super::*;

    fn model(id: &str, filename: &str) -> LocalModel {
        LocalModel {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            filename: filename.into(),
            url: String::new(),
            size_bytes: 0,
            downloaded: false,
            fits: true,
            fit_class: String::new(),
            family: String::new(),
            vision: false,
            builtin: false,
            vision_ready: false,
        }
    }

    /// Regression: custom models must be part of id resolution.
    /// (select_local_model used to search built-ins only.)
    #[test]
    fn custom_models_resolve() {
        let merged = merge_models(
            vec![model("builtin-a", "a.gguf")],
            vec![model("custom-hf", "custom.gguf")],
        );
        assert!(merged.iter().any(|m| m.id == "custom-hf"));
        assert!(merged.iter().any(|m| m.id == "builtin-a"));
    }

    /// Built-ins win on id collisions so a custom entry cannot shadow
    /// a shipped model.
    #[test]
    fn builtin_wins_on_collision() {
        let merged = merge_models(
            vec![model("gemma", "shipped.gguf")],
            vec![model("gemma", "impostor.gguf")],
        );
        let hits: Vec<_> = merged.iter().filter(|m| m.id == "gemma").collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].filename, "shipped.gguf");
    }
}

// ─────────────────────────────────────────────
//  HuggingFace repo resolution (W1.4)
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct HfGgufFile {
    pub filename: String,
    pub size_bytes: u64,
    /// Best-effort quant label parsed from the filename ("UD-Q3_K_M",
    /// "Q4_K_M", "BF16", ...). Empty when nothing recognizable.
    pub quant: String,
    /// Ready-to-download resolve URL.
    pub url: String,
    /// Multi-part GGUF (xxxxx-of-yyyyy). Not installable by the
    /// single-file downloader; shown but not selectable.
    pub split: bool,
}

/// Accepts "org/name", a full repo URL, or a resolve URL and returns the
/// canonical "org/name". Rejects anything else rather than guessing.
pub fn normalize_hf_repo(input: &str) -> Result<String, String> {
    let mut s = input.trim();
    for prefix in ["https://huggingface.co/", "http://huggingface.co/", "huggingface.co/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
            break;
        }
    }
    let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 {
        return Err("Expected a repo like 'org/model' or a huggingface.co URL".to_string());
    }
    let (org, name) = (parts[0], parts[1]);
    let ok = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !ok(org) || !ok(name) {
        return Err(format!("'{}/{}' is not a valid HuggingFace repo id", org, name));
    }
    Ok(format!("{}/{}", org, name))
}

/// Parse a quant label out of a GGUF filename, best effort.
pub fn quant_label(filename: &str) -> String {
    let stem = filename.trim_end_matches(".gguf");
    let upper = stem.to_uppercase();
    let tokens: Vec<&str> = upper.split(|c: char| c == '-' || c == '.').collect();
    for (i, t) in tokens.iter().enumerate() {
        let is_quant = (t.starts_with('Q') || t.starts_with("IQ"))
            && t.len() >= 2
            && t.as_bytes()[if t.starts_with("IQ") { 2 } else { 1 }].is_ascii_digit()
            || matches!(*t, "BF16" | "F16" | "F32" | "MXFP4_MOE" | "MXFP4");
        if is_quant {
            // Include a UD- prefix if present (unsloth dynamic quants)
            if i > 0 && tokens[i - 1] == "UD" {
                return format!("UD-{}", t);
            }
            return t.to_string();
        }
    }
    String::new()
}

fn is_split_gguf(filename: &str) -> bool {
    // xxxxx-of-yyyyy.gguf
    let stem = filename.trim_end_matches(".gguf");
    if let Some(idx) = stem.rfind("-of-") {
        let before = &stem[..idx];
        let after = &stem[idx + 4..];
        let part = before.rsplit('-').next().unwrap_or("");
        return part.len() == 5
            && part.chars().all(|c| c.is_ascii_digit())
            && after.len() == 5
            && after.chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// List the GGUF files of a HuggingFace repo with sizes, via the HF API
/// (?blobs=true includes per-file sizes). `token` unlocks gated/private
/// repos and is used for this request only - it is not stored.
pub async fn hf_list_gguf_files(
    repo_input: &str,
    token: Option<String>,
) -> Result<Vec<HfGgufFile>, String> {
    let repo = normalize_hf_repo(repo_input)?;
    let api_url = format!("https://huggingface.co/api/models/{}?blobs=true", repo);
    let client = reqwest::Client::new();
    let mut req = client.get(&api_url);
    if let Some(t) = token.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        req = req.bearer_auth(t);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("HuggingFace API request failed: {}", e))?;
    match resp.status().as_u16() {
        200 => {}
        401 | 403 => {
            return Err(format!(
                "Repo '{}' is gated or private. Provide a HuggingFace access token with read scope.",
                repo
            ))
        }
        404 => return Err(format!("Repo '{}' not found on HuggingFace", repo)),
        s => return Err(format!("HuggingFace API returned HTTP {}", s)),
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Could not parse HuggingFace API response: {}", e))?;
    let siblings = body
        .get("siblings")
        .and_then(|s| s.as_array())
        .ok_or("HuggingFace API response had no file list")?;
    let mut files: Vec<HfGgufFile> = siblings
        .iter()
        .filter_map(|s| {
            let name = s.get("rfilename")?.as_str()?;
            if !name.to_lowercase().ends_with(".gguf") {
                return None;
            }
            Some(HfGgufFile {
                filename: name.to_string(),
                size_bytes: s.get("size").and_then(|v| v.as_u64()).unwrap_or(0),
                quant: quant_label(name),
                url: format!("https://huggingface.co/{}/resolve/main/{}", repo, name),
                split: is_split_gguf(name),
            })
        })
        .collect();
    if files.is_empty() {
        return Err(format!("Repo '{}' contains no GGUF files", repo));
    }
    files.sort_by_key(|f| f.size_bytes);
    Ok(files)
}

#[cfg(test)]
mod hf_resolution_tests {
    use super::*;

    #[test]
    fn normalize_accepts_id_and_urls() {
        for input in [
            "unsloth/gemma-4-E4B-it-GGUF",
            "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF",
            "huggingface.co/unsloth/gemma-4-E4B-it-GGUF/tree/main",
            "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/x.gguf",
        ] {
            assert_eq!(
                normalize_hf_repo(input).unwrap(),
                "unsloth/gemma-4-E4B-it-GGUF",
                "input: {}",
                input
            );
        }
        assert!(normalize_hf_repo("just-a-name").is_err());
        assert!(normalize_hf_repo("bad org/name!").is_err());
    }

    #[test]
    fn quant_labels_parse() {
        assert_eq!(quant_label("gemma-4-26B-A4B-it-UD-Q3_K_M.gguf"), "UD-Q3_K_M");
        assert_eq!(quant_label("Qwen2.5-7B-Instruct-Q4_K_M.gguf"), "Q4_K_M");
        assert_eq!(quant_label("model-BF16.gguf"), "BF16");
        assert_eq!(quant_label("model-UD-IQ3_XXS.gguf"), "UD-IQ3_XXS");
        assert_eq!(quant_label("no-quant-here.gguf"), "");
    }

    #[test]
    fn split_files_detected() {
        assert!(is_split_gguf("q4-00001-of-00002.gguf"));
        assert!(!is_split_gguf("Qwen2.5-7B-Instruct-Q4_K_M.gguf"));
        assert!(!is_split_gguf("weird-of-name.gguf"));
    }
}

#[cfg(test)]
mod download_client_tests {
    /// Regression proof: `.timeout(Duration::from_secs(0))` is a ZERO
    /// deadline in reqwest, not "no timeout" - every request dies
    /// instantly. This is what silently broke all model downloads.
    #[test]
    #[ignore] // hits the network; run explicitly
    fn zero_timeout_kills_requests_instantly() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let err = rt.block_on(async {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(0))
                .build()
                .unwrap();
            client.get("https://huggingface.co").send().await.err().map(|e| e.to_string())
        });
        let msg = err.expect("zero-timeout request unexpectedly succeeded");
        assert!(msg.to_lowercase().contains("timed out") || msg.to_lowercase().contains("timeout"), "unexpected error: {}", msg);
    }
}

// Live check (FABLE follow-up): reclaim a real orphan on the port.
// Ignored by default; needs the llama-server binary present.
//   cargo test --bin orbixcore -- --ignored reclaim_live --nocapture
#[cfg(test)]
mod reclaim_live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn reclaim_live() {
        let dir = models_dir().parent().unwrap().join("bin").join("llama-b9969");
        let bin = dir.join("llama-server");
        if !bin.exists() { eprintln!("[skip] no llama-server binary"); return; }
        let model_dir = models_dir();
        let model = std::fs::read_dir(&model_dir).ok()
            .and_then(|rd| rd.flatten().map(|e| e.path())
                .find(|p| p.extension().map(|x| x == "gguf").unwrap_or(false)));
        let Some(model) = model else { eprintln!("[skip] no model"); return; };

        // Spawn an "orphan" on a test port.
        let port: u16 = 8859;
        let mut orphan = tokio::process::Command::new(&bin)
            .env("DYLD_LIBRARY_PATH", &dir)
            .args(["--model", model.to_str().unwrap(), "--port", &port.to_string(),
                   "--ctx-size", "4096", "--n-gpu-layers", "99", "--no-warmup", "--flash-attn", "on"])
            .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .spawn().unwrap();
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            if port_listener(port).is_some() { break; }
        }
        let (pid, cmd) = port_listener(port).expect("orphan should hold the port");
        eprintln!("[live] orphan pid={pid} cmd={cmd}");
        assert!(is_our_llama_server(&cmd), "our own binary must be reclaimable: {cmd}");

        reclaim_port(port).await.expect("reclaim should free the port");
        assert!(port_listener(port).is_none(), "port must be free after reclaim");
        eprintln!("[live] reclaim OK, port {port} free");
        let _ = orphan.kill().await; // no-op if already reaped
    }
}
