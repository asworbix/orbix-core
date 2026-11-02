use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use tauri::Emitter;

// ─────────────────────────────────────────────
//  Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct VoiceSettings {
    pub tts_voice: String,
    pub tts_rate: i32,
    pub auto_speak: bool,
    pub stt_language: String,
    #[serde(default = "default_stt_model")]
    pub stt_model: String,
}

fn default_stt_model() -> String { "base".to_string() }

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            tts_voice: "auto".to_string(), // Auto-pick best available Apple neural voice
            tts_rate: 175,
            auto_speak: false,
            stt_language: "en-US".to_string(),
            stt_model: default_stt_model(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TtsVoice {
    pub name: String,
    pub lang: String,
    pub gender: String,  // "female" | "male" | "other"
    pub provider: String, // "kokoro" | "system"
    pub display_name: String, // Human-friendly name
}

// ─────────────────────────────────────────────
//  Kokoro TTS integration
// ─────────────────────────────────────────────

/// Curated list of Kokoro voices - best English male/female options
pub fn kokoro_voices() -> Vec<TtsVoice> {
    vec![
        // Female voices (highest quality)
        TtsVoice {
            name: "af_heart".into(),
            display_name: "Heart (Female, US)".into(),
            lang: "en-US".into(),
            gender: "female".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "af_bella".into(),
            display_name: "Bella (Female, US)".into(),
            lang: "en-US".into(),
            gender: "female".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "af_nicole".into(),
            display_name: "Nicole (Female, US)".into(),
            lang: "en-US".into(),
            gender: "female".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "af_sky".into(),
            display_name: "Sky (Female, US)".into(),
            lang: "en-US".into(),
            gender: "female".into(),
            provider: "kokoro".into(),
        },
        // Male voices
        TtsVoice {
            name: "am_michael".into(),
            display_name: "Michael (Male, US)".into(),
            lang: "en-US".into(),
            gender: "male".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "am_fenrir".into(),
            display_name: "Fenrir (Male, US)".into(),
            lang: "en-US".into(),
            gender: "male".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "am_puck".into(),
            display_name: "Puck (Male, US)".into(),
            lang: "en-US".into(),
            gender: "male".into(),
            provider: "kokoro".into(),
        },
        // British English
        TtsVoice {
            name: "bf_emma".into(),
            display_name: "Emma (Female, UK)".into(),
            lang: "en-GB".into(),
            gender: "female".into(),
            provider: "kokoro".into(),
        },
        TtsVoice {
            name: "bm_george".into(),
            display_name: "George (Male, UK)".into(),
            lang: "en-GB".into(),
            gender: "male".into(),
            provider: "kokoro".into(),
        },
    ]
}

/// Global Kokoro TTS engine - lazy-initialized, cached for reuse.
/// The engine owns the ONNX model and voice embeddings.
static KOKORO_ENGINE: std::sync::OnceLock<tokio::sync::Mutex<Option<kokoro_micro::TtsEngine>>> = std::sync::OnceLock::new();

/// Global "please stop" flag - set when stop_audio is called, checked by playback thread.
static AUDIO_STOP_FLAG: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

fn get_engine_lock() -> &'static tokio::sync::Mutex<Option<kokoro_micro::TtsEngine>> {
    KOKORO_ENGINE.get_or_init(|| tokio::sync::Mutex::new(None))
}

fn get_stop_flag() -> Arc<AtomicBool> {
    AUDIO_STOP_FLAG.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

/// Get or initialize the Kokoro engine. First call downloads the model (~340MB).
pub async fn ensure_kokoro_engine() -> Result<(), String> {
    let lock = get_engine_lock();
    let mut guard = lock.lock().await;
    if guard.is_none() {
        // First-time init - downloads model files to ~/.cache/k/
        let engine = kokoro_micro::TtsEngine::new()
            .await
            .map_err(|e| format!("Kokoro init failed: {}", e))?;
        *guard = Some(engine);
    }
    Ok(())
}

/// Check if Kokoro model is already downloaded
pub fn kokoro_model_downloaded() -> bool {
    let cache = dirs::home_dir()
        .map(|h| h.join(".cache").join("k"))
        .unwrap_or_default();
    cache.join("0.onnx").exists() && cache.join("0.bin").exists()
}

/// Synthesize text using Kokoro, returns f32 audio samples at 24000 Hz
pub async fn kokoro_synthesize(text: &str, voice: &str, speed: f32) -> Result<Vec<f32>, String> {
    ensure_kokoro_engine().await?;
    let lock = get_engine_lock();
    let mut guard = lock.lock().await;
    let engine = guard.as_mut().ok_or("Engine not initialized")?;

    let samples = engine
        .synthesize_with_options(text, Some(voice), speed, 1.0, Some("en"))
        .map_err(|e| format!("Kokoro synthesis failed: {}", e))?;

    Ok(samples)
}

/// Play audio samples through the default output device and BLOCK until playback finishes.
/// rodio types aren't Send, so we spawn a dedicated thread that owns the OutputStream,
/// then wait on a channel for completion. This lets callers (and the TTS state machine)
/// know exactly when audio has finished playing.
///
/// When `app` is Some, emits `tts-amplitude` events (f32, 0.0-1.0) every ~33ms synced to
/// playback position - this powers the reactive orb on the frontend.
pub fn play_samples(samples: Vec<f32>, sample_rate: u32, app: Option<tauri::AppHandle>) -> Result<(), String> {
    let stop_flag = get_stop_flag();
    stop_flag.store(false, Ordering::SeqCst);

    // Pre-compute per-frame RMS windows (30 fps → sample_rate / 30 samples per window).
    // We emit these on a timer so the orb pulses in lockstep with audible playback.
    let window_size = (sample_rate / 30).max(1) as usize;
    let mut rms_windows: Vec<f32> = Vec::with_capacity(samples.len() / window_size + 1);
    {
        let mut i = 0;
        while i < samples.len() {
            let end = (i + window_size).min(samples.len());
            let chunk = &samples[i..end];
            let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
            let rms = (sum_sq / chunk.len() as f32).sqrt();
            // Normalize: typical speech peaks around 0.3 RMS → map to ~1.0 for orb max
            let level = (rms * 3.5).clamp(0.0, 1.0);
            rms_windows.push(level);
            i += window_size;
        }
    }

    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let app_for_amp = app.clone();
    let stop_flag_for_amp = stop_flag.clone();

    // Amplitude emitter thread - runs in parallel with rodio playback at ~30fps
    // with hysteresis: skip emissions where the level hasn't changed meaningfully
    // from the last sent value (cuts event traffic ~50% during typical speech).
    if let Some(app_h) = app_for_amp {
        std::thread::spawn(move || {
            const HYSTERESIS: f32 = 0.015;
            let mut last_sent: f32 = -1.0; // force first emit
            for level in rms_windows {
                if stop_flag_for_amp.load(Ordering::SeqCst) {
                    let _ = crate::event_bus::emit_ui(&app_h, "tts-amplitude", 0.0_f32);
                    return;
                }
                if (level - last_sent).abs() >= HYSTERESIS {
                    let _ = crate::event_bus::emit_ui(&app_h, "tts-amplitude", level);
                    last_sent = level;
                }
                std::thread::sleep(std::time::Duration::from_millis(33));
            }
            let _ = crate::event_bus::emit_ui(&app_h, "tts-amplitude", 0.0_f32);
        });
    }

    std::thread::spawn(move || {
        use rodio::{OutputStream, Sink};
        let (_stream, handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => { eprintln!("[voice] Audio output init failed: {}", e); let _ = tx.send(()); return; }
        };
        let sink = match Sink::try_new(&handle) {
            Ok(s) => s,
            Err(e) => { eprintln!("[voice] Sink creation failed: {}", e); let _ = tx.send(()); return; }
        };

        let source = rodio::buffer::SamplesBuffer::new(1, sample_rate, samples);
        sink.append(source);

        while !sink.empty() {
            if stop_flag.load(Ordering::SeqCst) {
                sink.stop();
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = tx.send(());
    });

    let _ = rx.recv();
    // Final zero in case playback ended before the amp thread finished (Kokoro windows cap short)
    if let Some(app_h) = app {
        let _ = crate::event_bus::emit_ui(&app_h, "tts-amplitude", 0.0_f32);
    }
    Ok(())
}

/// Stop any currently playing audio
pub fn stop_audio() {
    get_stop_flag().store(true, Ordering::SeqCst);
}

pub struct RecordingState {
    pub is_recording: AtomicBool,
    pub samples: Arc<Mutex<Vec<f32>>>,
    pub sample_rate: Mutex<u32>,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: Mutex::new(44100),
        }
    }
}

// ─────────────────────────────────────────────
//  TTS - macOS `say` command
// ─────────────────────────────────────────────

/// Clean text for TTS - strips markdown, code blocks, action tags, and
/// other non-spoken characters so the synthesizer isn't trying to phonemize
/// JSON braces or asterisks.
pub fn clean_for_tts(text: &str) -> String {
    let mut s = text.to_string();

    // Remove <action>...</action> blocks (and multiline)
    let re_action = regex::Regex::new(r"(?s)<action>.*?</action>").unwrap();
    s = re_action.replace_all(&s, "").to_string();

    // Remove fenced code blocks
    let re_code_fence = regex::Regex::new(r"(?s)```[^`]*```").unwrap();
    s = re_code_fence.replace_all(&s, "").to_string();

    // Remove inline code
    let re_inline_code = regex::Regex::new(r"`[^`]*`").unwrap();
    s = re_inline_code.replace_all(&s, "").to_string();

    // Strip markdown headers (## Title)
    let re_header = regex::Regex::new(r"(?m)^#{1,6}\s+").unwrap();
    s = re_header.replace_all(&s, "").to_string();

    // Strip bold/italic markers - keep the text inside
    let re_bold = regex::Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    s = re_bold.replace_all(&s, "$1").to_string();
    let re_italic = regex::Regex::new(r"\*([^*]+)\*").unwrap();
    s = re_italic.replace_all(&s, "$1").to_string();
    let re_under = regex::Regex::new(r"_([^_]+)_").unwrap();
    s = re_under.replace_all(&s, "$1").to_string();

    // Strip markdown links: [text](url) -> text
    let re_link = regex::Regex::new(r"\[([^\]]+)\]\([^)]*\)").unwrap();
    s = re_link.replace_all(&s, "$1").to_string();

    // Strip remaining HTML/XML tags
    let re_tags = regex::Regex::new(r"<[^>]+>").unwrap();
    s = re_tags.replace_all(&s, "").to_string();

    // Bullet markers at line start
    let re_bullet = regex::Regex::new(r"(?m)^\s*[-*•]\s+").unwrap();
    s = re_bullet.replace_all(&s, "").to_string();

    // Numbered list markers
    let re_numlist = regex::Regex::new(r"(?m)^\s*\d+\.\s+").unwrap();
    s = re_numlist.replace_all(&s, "").to_string();

    // Horizontal rules
    let re_hr = regex::Regex::new(r"(?m)^---+$").unwrap();
    s = re_hr.replace_all(&s, "").to_string();

    // Bob's signature
    s = s.replace("- Bob ◉", "").replace("Bob ◉", "");

    // Collapse multiple newlines
    let re_nl = regex::Regex::new(r"\n{2,}").unwrap();
    s = re_nl.replace_all(&s, ". ").to_string();

    // Collapse single newlines into spaces
    s = s.replace('\n', " ");

    // Collapse multiple spaces
    let re_ws = regex::Regex::new(r" {2,}").unwrap();
    s = re_ws.replace_all(&s, " ").to_string();

    s.trim().to_string()
}

/// Detect if a voice name is a Kokoro voice (af_, am_, bf_, bm_, etc.)
fn is_kokoro_voice(voice: &str) -> bool {
    let v = voice.to_lowercase();
    v.starts_with("af_") || v.starts_with("am_") ||
    v.starts_with("bf_") || v.starts_with("bm_") ||
    v.starts_with("ef_") || v.starts_with("em_") ||
    v.starts_with("ff_") || v.starts_with("fm_") ||
    v.starts_with("jf_") || v.starts_with("jm_") ||
    v.starts_with("zf_") || v.starts_with("zm_")
}

/// Resolve the path to the bundled orbix-speak binary. It ships as a
/// sidecar (bundle.externalBin) next to the app executable, in the bundle
/// (Contents/MacOS) and under `tauri dev` alike, signed with the app.
/// Older bundles staged it as a resource; development falls back to the
/// staged sidecar in the crate, then the bare compile in swift/.
#[cfg(target_os = "macos")]
fn orbix_speak_path() -> PathBuf {
    let real = |p: &PathBuf| std::fs::metadata(p).map(|m| m.is_file() && m.len() > 1024).unwrap_or(false);
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        for rel in ["orbix-speak", "../Resources/swift/orbix-speak", "../Resources/orbix-speak"] {
            let p = dir.join(rel);
            if real(&p) { return p; }
        }
    }
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for p in [
        crate_dir.join(format!("binaries/orbix-speak-{}", env!("ORBIX_TARGET_TRIPLE"))),
        crate_dir.join("swift/orbix-speak"),
    ] {
        if real(&p) { return p; }
    }
    // Last resort
    PathBuf::from("orbix-speak")
}

// PID of the active TTS child process so stop_speaking can kill it.
// 0 means "no active child".
#[cfg(target_os = "windows")]
static ACTIVE_TTS_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(target_os = "macos")]
pub async fn speak(text: &str, voice: &str, rate: i32, app: Option<tauri::AppHandle>) -> Result<(), String> {
    // Strip markdown / action tags / code blocks before TTS.
    let cleaned = clean_for_tts(text);
    if cleaned.is_empty() {
        return Ok(());
    }
    eprintln!("[voice] TTS input: {} chars (cleaned from {})", cleaned.len(), text.len());

    // Emit tts-state so the frontend knows Bob is speaking
    if let Some(ref a) = app {
        crate::event_bus::emit_ui(a, "tts-state", "speaking").ok();
    }

    // ── AVSpeechSynthesizer path via orbix-speak binary ──
    // Uses Apple's neural voices (same engine as Siri). Streaming start,
    // natural pacing, zero RAM overhead. Replaces Kokoro entirely.
    //
    // Voice mapping:
    // - Kokoro names (af_heart, am_michael, etc.) → "auto" (best available)
    // - System voice names → passed through as-is
    // - "auto" → picks premium > enhanced > default English voice
    let voice_arg = if is_kokoro_voice(voice) { "auto" } else { voice };

    // Rate mapping: tts_rate 120-250 → AVSpeech 0.40-0.58
    // AVSpeechSynthesizer's default rate is 0.5. Range 0.35-0.65 is natural.
    let av_rate = (0.40 + (rate as f32 - 120.0) * (0.18 / 130.0)).clamp(0.38, 0.60);

    let speak_bin = orbix_speak_path();
    eprintln!("[voice] Using orbix-speak at: {:?}", speak_bin);

    let status = tokio::process::Command::new(&speak_bin)
        .arg(voice_arg)
        .arg(format!("{:.2}", av_rate))
        .arg(&cleaned)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .status()
        .await;

    if let Some(ref a) = app {
        crate::event_bus::emit_ui(a, "tts-state", "idle").ok();
    }

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("[voice] orbix-speak exited with: {}", s);
            let system_voice = if is_kokoro_voice(voice) { "Samantha" } else { voice };
            let _ = tokio::process::Command::new("say")
                .arg("-v").arg(system_voice)
                .arg("-r").arg(rate.to_string())
                .arg(&cleaned)
                .status().await;
            Ok(())
        }
        Err(e) => {
            eprintln!("[voice] orbix-speak not found ({}), falling back to say", e);
            let system_voice = if is_kokoro_voice(voice) { "Samantha" } else { voice };
            let _ = tokio::process::Command::new("say")
                .arg("-v").arg(system_voice)
                .arg("-r").arg(rate.to_string())
                .arg(&cleaned)
                .status().await;
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
pub async fn stop_speaking() -> Result<(), String> {
    stop_audio();
    tokio::process::Command::new("pkill")
        .arg("-f").arg("orbix-speak")
        .status().await.ok();
    tokio::process::Command::new("killall")
        .arg("say")
        .status().await.ok();
    Ok(())
}

// ── Windows TTS via PowerShell SAPI (System.Speech.Synthesis) ──
// SAPI ships with every Windows install; no extra deps. We drive it through
// powershell.exe with a base64-encoded UTF-16LE -EncodedCommand to avoid
// any quoting/escaping issues with arbitrary Unicode in the speech text.
#[cfg(target_os = "windows")]
fn build_sapi_script(text: &str, voice: &str, rate: i32) -> String {
    // Map our 120..250 wpm rate range onto SAPI's -10..10 integer scale.
    // SAPI rate 0 ≈ 200 wpm, ±1 ≈ ±10 wpm.
    let sapi_rate = (((rate as f32 - 200.0) / 10.0).round() as i32).clamp(-10, 10);

    // Embed text as a single-quoted PowerShell literal with '' escaping for '.
    let escaped_text = text.replace('\'', "''");
    let voice_arg = voice.replace('\'', "''");

    format!(
        r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$synth.Rate = {rate}
if ('{voice}' -ne 'auto' -and '{voice}' -ne '') {{
    try {{ $synth.SelectVoice('{voice}') }} catch {{}}
}}
$synth.Speak('{text}')
$synth.Dispose()
"#,
        rate = sapi_rate,
        voice = voice_arg,
        text = escaped_text,
    )
}

#[cfg(target_os = "windows")]
fn encode_powershell_command(script: &str) -> String {
    use base64::Engine;
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for u in utf16 {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(target_os = "windows")]
pub async fn speak(text: &str, voice: &str, rate: i32, app: Option<tauri::AppHandle>) -> Result<(), String> {
    let cleaned = clean_for_tts(text);
    if cleaned.is_empty() {
        return Ok(());
    }
    eprintln!("[voice] TTS input: {} chars (cleaned from {})", cleaned.len(), text.len());

    if let Some(ref a) = app {
        a.emit("tts-state", "speaking").ok();
    }

    // Kokoro voice ids don't exist on Windows SAPI - pick the system default.
    let voice_arg = if is_kokoro_voice(voice) || voice == "auto" { "" } else { voice };

    let script = build_sapi_script(&cleaned, voice_arg, rate);
    let encoded = encode_powershell_command(&script);

    // Spawn powershell so we can track the child for stop_speaking().
    let spawn_result = crate::process_ext::tokio_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            if let Some(ref a) = app { a.emit("tts-state", "idle").ok(); }
            return Err(format!("powershell spawn failed: {}", e));
        }
    };

    // Track PID so stop_speaking() can taskkill it.
    if let Some(pid) = child.id() {
        ACTIVE_TTS_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
    }

    let status = child.wait().await;

    ACTIVE_TTS_PID.store(0, std::sync::atomic::Ordering::SeqCst);

    if let Some(ref a) = app {
        a.emit("tts-state", "idle").ok();
    }

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => {
            eprintln!("[voice] powershell SAPI exited with: {}", s);
            Ok(())
        }
        Err(e) => Err(format!("powershell wait failed: {}", e)),
    }
}

#[cfg(target_os = "windows")]
pub async fn stop_speaking() -> Result<(), String> {
    stop_audio();
    let pid = ACTIVE_TTS_PID.swap(0, std::sync::atomic::Ordering::SeqCst);
    if pid != 0 {
        // /T also kills the SAPI child process tree (the audio render thread
        // is in-process, so the parent kill is enough - but /T is harmless).
        let _ = crate::process_ext::tokio_command("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .status()
            .await;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub async fn list_voices() -> Result<Vec<TtsVoice>, String> {
    // Premium only (UX): the curated Kokoro neural voices come first (all
    // high-quality neural), then Auto (picks the best Apple neural voice),
    // then only Apple voices flagged (Premium) or (Enhanced). The old list
    // dumped every macOS voice via `say -v ?`, including the novelty ones
    // (Bad News, Bubbles, Zarvox, Trinoids, ...) and low-quality defaults -
    // and it never even offered the premium Kokoro voices.
    let mut voices: Vec<TtsVoice> = kokoro_voices();
    voices.push(TtsVoice {
        name: "auto".to_string(),
        lang: "en-US".to_string(),
        gender: "female".to_string(),
        provider: "apple-neural".to_string(),
        display_name: "Auto (Best Apple Neural Voice)".to_string(),
    });

    // Append only PREMIUM / ENHANCED Apple voices (English).
    let output = tokio::process::Command::new("say")
        .arg("-v")
        .arg("?")
        .output()
        .await
        .map_err(|e| format!("Failed to list system voices: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        // The name is everything up to the 2+ space gap before the locale.
        let (raw_name, rest) = match line.find("  ") {
            Some(i) => (line[..i].trim(), line[i..].trim()),
            None => continue,
        };
        // Keep only high-quality voices: annotated (Premium) or (Enhanced).
        let quality_ok = raw_name.contains("(Premium)") || raw_name.contains("(Enhanced)");
        let is_english = rest.starts_with("en_") || rest.starts_with("en-");
        if !quality_ok || !is_english {
            continue;
        }
        // Bare voice name for the `say`/AVSpeech identifier (strip the
        // "(Premium)"/"(Enhanced)" tag; `say -v "Ava (Premium)"` also works,
        // so keep the full name for selection).
        let name = raw_name.to_string();
        if name.is_empty() { continue; }
        let lang = rest
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .replace('_', "-")
            .to_string();
        // Best-effort gender detection from the base name (strip the
        // "(Premium)"/"(Enhanced)" and any "(English (US))" annotations).
        let base_name = name.split(" (").next().unwrap_or(&name);
        let gender = match base_name {
            "Samantha" | "Karen" | "Moira" | "Tessa" | "Victoria" | "Fiona" | "Ava" | "Allison"
            | "Susan" | "Serena" | "Kate" | "Emily" | "Veena" | "Paulina" | "Amelie" | "Anna"
            | "Carmit" | "Damayanti" | "Ellen" | "Ioana" | "Joana" | "Kanya" | "Kyoko" | "Laura"
            | "Lekha" | "Luciana" | "Mariska" | "Mei-Jia" | "Melina" | "Milena" | "Monica"
            | "Nora" | "Princess" | "Sara" | "Satu" | "Sin-ji" | "Tian-Tian" | "Ting-Ting"
            | "Xander" | "Yelda" | "Yuna" | "Zosia" | "Zuzana" => "female",
            "Alex" | "Daniel" | "Diego" | "Fred" | "Jorge" | "Juan" | "Luca" | "Maged"
            | "Martin" | "Oliver" | "Ralph" | "Thomas" | "Tom" => "male",
            _ => "other",
        };
        voices.push(TtsVoice {
            name: name.clone(),
            display_name: format!("{} ({})", name, lang.split('-').next().unwrap_or(&lang)),
            lang,
            gender: gender.to_string(),
            provider: "system".to_string(),
        });
    }

    Ok(voices)
}

#[cfg(target_os = "windows")]
pub async fn list_voices() -> Result<Vec<TtsVoice>, String> {
    let mut voices = vec![TtsVoice {
        name: "auto".to_string(),
        lang: "en-US".to_string(),
        gender: "female".to_string(),
        provider: "system".to_string(),
        display_name: "Auto (System Default)".to_string(),
    }];

    // Enumerate installed SAPI voices via PowerShell.
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
foreach ($v in $s.GetInstalledVoices()) {
    $info = $v.VoiceInfo
    Write-Output ("{0}|{1}|{2}" -f $info.Name, $info.Culture.Name, $info.Gender)
}
$s.Dispose()
"#;
    let encoded = encode_powershell_command(script);
    let output = crate::process_ext::tokio_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .output()
        .await
        .map_err(|e| format!("Failed to enumerate SAPI voices: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let parts: Vec<&str> = line.trim().splitn(3, '|').collect();
        if parts.len() < 3 { continue; }
        let name = parts[0].trim().to_string();
        if name.is_empty() { continue; }
        let lang = parts[1].trim().to_string();
        let gender = match parts[2].trim().to_lowercase().as_str() {
            "female" => "female",
            "male" => "male",
            _ => "other",
        }.to_string();
        let lang_short = lang.split('-').next().unwrap_or(&lang);
        voices.push(TtsVoice {
            display_name: format!("{} ({})", name, lang_short),
            name,
            lang,
            gender,
            provider: "system".to_string(),
        });
    }

    Ok(voices)
}

// ─────────────────────────────────────────────
//  STT - cpal recording + macOS SFSpeechRecognizer
// ─────────────────────────────────────────────

pub fn start_recording(recording: &Arc<RecordingState>, app: tauri::AppHandle) -> Result<cpal::Stream, String> {
    eprintln!("[voice] start_recording called");
    let app_for_partial = app.clone();
    recording.is_recording.store(true, Ordering::SeqCst);
    {
        let mut samples = recording.samples.lock().unwrap_or_else(|e| e.into_inner());
        samples.clear();
    }

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No audio input device available")?;

    let device_name = device.name().unwrap_or_else(|_| "unknown".to_string());
    eprintln!("[voice] Using input device: {}", device_name);

    let config = device
        .default_input_config()
        .map_err(|e| format!("Failed to get input config: {}", e))?;

    eprintln!("[voice] Input config: {} Hz, {} channels, format={:?}",
        config.sample_rate().0, config.channels(), config.sample_format());

    {
        let mut sr = recording.sample_rate.lock().unwrap_or_else(|e| e.into_inner());
        *sr = config.sample_rate().0;
    }

    let samples_clone = Arc::clone(&recording.samples);
    let is_recording = Arc::new(AtomicBool::new(true));
    let is_rec_clone = Arc::clone(&is_recording);
    recording.is_recording.store(true, Ordering::SeqCst);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_input_stream::<f32>(&device, &config.into(), samples_clone, is_rec_clone, app),
        cpal::SampleFormat::I16 => build_input_stream::<i16>(&device, &config.into(), samples_clone, is_rec_clone, app),
        cpal::SampleFormat::U16 => build_input_stream::<u16>(&device, &config.into(), samples_clone, is_rec_clone, app),
        _ => return Err("Unsupported sample format".to_string()),
    }?;

    stream.play().map_err(|e| format!("Failed to start recording: {}", e))?;

    // ── Live partial transcripts ──
    // While recording, transcribe the audio-so-far on an interval and emit
    // `stt-partial` so the user sees words appear as they speak instead of
    // only after they stop. The final stop_listening path still does the
    // authoritative full transcription. Runs sequentially (each whisper pass
    // is awaited) so passes never pile up; whisper is only nudged once there
    // is enough new audio to be worth it.
    {
        let rec = Arc::clone(recording);
        let app_p = app_for_partial;
        tauri::async_runtime::spawn(async move {
            let mut last_len = 0usize;
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
                if !rec.is_recording.load(Ordering::SeqCst) {
                    break;
                }
                let (snapshot, rate) = {
                    let samples = rec.samples.lock().unwrap_or_else(|e| e.into_inner());
                    let rate = *rec.sample_rate.lock().unwrap_or_else(|e| e.into_inner());
                    (samples.clone(), rate)
                };
                // Need at least ~0.6s of audio, and meaningful new audio since
                // the last pass, to bother re-running whisper.
                let min_new = (rate as usize) / 3; // ~0.33s of new samples
                if snapshot.len() < (rate as usize) / 2 || snapshot.len().saturating_sub(last_len) < min_new {
                    continue;
                }
                last_len = snapshot.len();
                if let Ok(text) = transcribe_samples(snapshot, rate, "en").await {
                    if !text.is_empty() {
                        crate::event_bus::emit_ui(&app_p, "stt-partial", serde_json::json!({ "text": text })).ok();
                    }
                }
            }
        });
    }

    Ok(stream)
}

fn build_input_stream<T: cpal::Sample + cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    _is_recording: Arc<AtomicBool>,
    app: tauri::AppHandle,
) -> Result<cpal::Stream, String>
where
    f32: cpal::FromSample<T>,
{
    let err_fn = |err: cpal::StreamError| {
        eprintln!("Audio stream error: {}", err);
    };

    // Emit audio levels at ~30fps (throttle by frame count)
    let sample_rate = config.sample_rate.0;
    let emit_interval = (sample_rate / 30) as usize; // samples per ~33ms

    // VAD state: tuned for natural conversation pace.
    // - SILENCE_THRESHOLD: RMS level below this counts as silence
    // - SILENCE_FRAMES_TO_STOP: consecutive silent frames (at 30fps) before auto-stop
    // - MIN_SPEECH_FRAMES: minimum frames of actual speech required before silence can stop recording
    // - STARTUP_GRACE_FRAMES: initial frames where VAD is ignored
    //
    // Silence tolerance was previously 800ms which cut off mid-sentence
    // thinking pauses. Bumped to ~1.6 seconds - matches the natural pause
    // length of a human mid-thought without feeling like the system stalled.
    const SILENCE_THRESHOLD: f32 = 0.012;
    const SILENCE_FRAMES_TO_STOP: usize = 48; // ~1.6s at 30fps - tolerates natural pauses
    const MIN_SPEECH_FRAMES: usize = 9;       // ~300ms of speech required
    const STARTUP_GRACE_FRAMES: usize = 18;   // ~600ms grace at startup
    let silence_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let speech_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let frame_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let silence_clone = Arc::clone(&silence_counter);
    let speech_clone = Arc::clone(&speech_counter);
    let frame_clone = Arc::clone(&frame_counter);
    let vad_emitted = Arc::new(AtomicBool::new(false));
    let vad_emitted_clone = Arc::clone(&vad_emitted);
    // Hysteresis: skip emitting audio-level if it hasn't moved meaningfully.
    // Cuts ~50% of frontend re-renders during typical voice activity.
    let last_emitted_level = Arc::new(Mutex::new(-1.0_f32));
    let last_emitted_clone = Arc::clone(&last_emitted_level);

    let stream = device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut buf = samples.lock().unwrap_or_else(|e| e.into_inner());
                let mut rms_sum: f32 = 0.0;
                for &sample in data {
                    let s: f32 = cpal::Sample::from_sample(sample);
                    buf.push(s);
                    rms_sum += s * s;
                }

                // Emit audio level periodically + run VAD
                if buf.len() % emit_interval < data.len() {
                    let rms = (rms_sum / data.len() as f32).sqrt();
                    // Normalized 0-1 for UI waveform
                    let level = (rms * 5.0).min(1.0);
                    // Hysteresis: only emit if level moved >= 0.015 from last sent
                    {
                        let mut last = last_emitted_clone.lock().unwrap_or_else(|e| e.into_inner());
                        if (level - *last).abs() >= 0.015 {
                            *last = level;
                            crate::event_bus::emit_ui(&app, "audio-level", level).ok();
                        }
                    }

                    let frame = frame_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // VAD: is this frame silent or speech?
                    if rms < SILENCE_THRESHOLD {
                        silence_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        silence_clone.store(0, std::sync::atomic::Ordering::Relaxed);
                        speech_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }

                    // Ignore VAD during startup grace period - give user time to start speaking
                    if frame < STARTUP_GRACE_FRAMES {
                        return;
                    }

                    // Auto-stop when sustained silence after actual speech
                    let silent = silence_clone.load(std::sync::atomic::Ordering::Relaxed);
                    let spoken = speech_clone.load(std::sync::atomic::Ordering::Relaxed);
                    if silent >= SILENCE_FRAMES_TO_STOP && spoken >= MIN_SPEECH_FRAMES {
                        // Emit once - frontend listens and calls stop_listening
                        if !vad_emitted_clone.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            crate::event_bus::emit_ui(&app, "silence-detected", ()).ok();
                        }
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

    Ok(stream)
}

pub fn stop_recording_and_save(recording: &Arc<RecordingState>) -> Result<PathBuf, String> {
    recording.is_recording.store(false, Ordering::SeqCst);

    let samples = recording.samples.lock().unwrap_or_else(|e| e.into_inner());
    let sample_rate = *recording.sample_rate.lock().unwrap_or_else(|e| e.into_inner());

    eprintln!("[voice] stop_recording_and_save: {} samples captured at {} Hz",
        samples.len(), sample_rate);

    if samples.is_empty() {
        return Err("No audio recorded - microphone may not be granting permission".to_string());
    }

    // Quick peak check - if audio is all silence, warn
    let peak = samples.iter().fold(0.0f32, |acc, &s| acc.max(s.abs()));
    eprintln!("[voice] Audio peak level: {:.4} (near 0 = silence, 0.1+ = speech)", peak);
    if peak < 0.001 {
        return Err(format!("Captured audio is silent (peak={:.4}) - check microphone input", peak));
    }

    let path = std::env::temp_dir().join("orbixcore_recording.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let file = std::fs::File::create(&path).map_err(|e| format!("Failed to create WAV: {}", e))?;
    let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)
        .map_err(|e| format!("Failed to write WAV header: {}", e))?;

    // Convert f32 samples to i16 (mono - take every channel's first sample if stereo)
    for &sample in samples.iter() {
        let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(s).map_err(|e| format!("WAV write error: {}", e))?;
    }

    writer.finalize().map_err(|e| format!("WAV finalize error: {}", e))?;
    Ok(path)
}

// ─────────────────────────────────────────────
//  Whisper.cpp STT - fully local, no system permissions beyond mic
// ─────────────────────────────────────────────

// Base model - ~140MB, roughly 2x the accuracy of tiny for only
// ~2x the inference time. Still real-time on any Mac since M1.
// Tiny was producing too many transcription errors for natural conversation.
// Whisper STT model registry (UX: fast vs accurate). ids: "tiny" (fastest,
// ~78MB), "base" (balanced, default, ~148MB), "small" (most accurate,
// ~488MB). All English-only. The choice is persisted next to the models.
fn whisper_model_info(id: &str) -> (&'static str, &'static str) {
    match id {
        "tiny" => ("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin", "ggml-tiny.en.bin"),
        "small" => ("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin", "ggml-small.en.bin"),
        // Multilingual + near-large accuracy at ~8x large's speed. The only
        // option that actually honours non-English stt_language settings
        // (the .en models are English-only).
        "turbo" => ("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin", "ggml-large-v3-turbo.bin"),
        _ => ("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin", "ggml-base.en.bin"),
    }
}

/// The persisted "which whisper model" choice.
static WHISPER_CHOICE: std::sync::OnceLock<Mutex<String>> = std::sync::OnceLock::new();

fn whisper_dir() -> PathBuf {
    crate::storage::base_dir().join("whisper")
}

fn whisper_choice_lock() -> &'static Mutex<String> {
    WHISPER_CHOICE.get_or_init(|| {
        // Default from a persisted file, else "base".
        let saved = std::fs::read_to_string(whisper_dir().join("model.txt"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| matches!(s.as_str(), "tiny" | "base" | "small" | "turbo"))
            .unwrap_or_else(|| "base".to_string());
        Mutex::new(saved)
    })
}

pub fn current_whisper_model() -> String {
    whisper_choice_lock().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Switch the STT model. Persists the choice and resets the loaded whisper
/// context so the next transcription loads (and downloads if needed) the new
/// model. No-op if unchanged.
pub fn set_whisper_model(id: &str) {
    let id = if matches!(id, "tiny" | "base" | "small" | "turbo") { id } else { "base" };
    {
        let mut choice = whisper_choice_lock().lock().unwrap_or_else(|e| e.into_inner());
        if *choice == id { return; }
        *choice = id.to_string();
    }
    let _ = std::fs::create_dir_all(whisper_dir());
    let _ = crate::storage::atomic_write(whisper_dir().join("model.txt"), id.as_bytes());
    // Drop the loaded context so the next ensure_whisper_loaded reloads the
    // newly-selected model.
    if let Some(lock) = WHISPER_CONTEXT.get() {
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

/// True if the whisper STT model is already on disk (so prewarm never
/// triggers the ~140MB download).
pub fn whisper_model_downloaded() -> bool {
    whisper_model_path().exists()
}

fn whisper_model_path() -> PathBuf {
    let (_, filename) = whisper_model_info(&current_whisper_model());
    whisper_dir().join(filename)
}

/// Download the whisper.cpp model if not already present (~150MB)
async fn ensure_whisper_model() -> Result<PathBuf, String> {
    let path = whisper_model_path();
    if path.exists() {
        return Ok(path);
    }
    eprintln!("[voice] Whisper model not found, downloading from HuggingFace...");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create whisper dir: {}", e))?;
    }
    let (url, _) = whisper_model_info(&current_whisper_model());
    let resp = reqwest::get(url).await
        .map_err(|e| format!("Whisper download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Whisper download HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().await
        .map_err(|e| format!("Whisper download read failed: {}", e))?;
    crate::storage::atomic_write(&path, &bytes)
        .map_err(|e| format!("Whisper write failed: {}", e))?;
    eprintln!("[voice] Whisper model downloaded ({} MB)", bytes.len() / 1024 / 1024);
    Ok(path)
}

/// Global Whisper context (loaded once, reused)
static WHISPER_CONTEXT: std::sync::OnceLock<Mutex<Option<whisper_rs::WhisperContext>>> = std::sync::OnceLock::new();

fn get_whisper_lock() -> &'static Mutex<Option<whisper_rs::WhisperContext>> {
    WHISPER_CONTEXT.get_or_init(|| Mutex::new(None))
}

/// Serializes context creation. The boot preload and the front end's
/// permission pre-request both land here within the same second; without
/// the gate both saw an empty slot and each loaded the model (the log
/// showed "whisper_model_load" twice), doubling load time and memory.
static WHISPER_LOAD_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn ensure_whisper_loaded() -> Result<(), String> {
    let _gate = WHISPER_LOAD_GATE.lock().await;
    {
        let lock = get_whisper_lock();
        if lock.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
            return Ok(());
        }
    }
    let model_path = ensure_whisper_model().await?;
    let path_str = model_path.to_string_lossy().to_string();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let ctx = whisper_rs::WhisperContext::new_with_params(
            &path_str,
            whisper_rs::WhisperContextParameters::default(),
        ).map_err(|e| format!("Whisper context init failed: {}", e))?;
        let lock = get_whisper_lock();
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
        eprintln!("[voice] Whisper context loaded");
        Ok(())
    }).await.map_err(|e| format!("Whisper init task failed: {}", e))??;
    Ok(())
}

/// Pre-request voice permissions on app boot. With whisper.cpp we only need
/// microphone access - no Apple speech recognition permission required.
pub async fn request_voice_permissions() -> Result<(bool, bool), String> {
    eprintln!("[voice] Pre-requesting mic permission");

    // Trigger microphone permission via cpal
    let mic_granted = tokio::task::spawn_blocking(|| {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => return false,
        };
        match device.default_input_config() {
            Ok(config) => {
                let stream_result: Result<cpal::Stream, _> = device.build_input_stream(
                    &config.into(),
                    |_data: &[f32], _: &cpal::InputCallbackInfo| {},
                    |_| {},
                    Some(std::time::Duration::from_millis(100)),
                );
                if let Ok(stream) = stream_result {
                    let _ = stream.play();
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    drop(stream);
                    return true;
                }
                false
            }
            Err(e) => {
                eprintln!("[voice] Mic permission probe failed: {}", e);
                false
            }
        }
    }).await.map_err(|e| format!("Permission probe task failed: {}", e))?;

    eprintln!("[voice] Microphone: {}", if mic_granted { "granted" } else { "DENIED" });

    // Pre-warm Whisper in the background - non-blocking
    tokio::spawn(async {
        if let Err(e) = ensure_whisper_loaded().await {
            eprintln!("[voice] Whisper preload failed: {}", e);
        }
    });

    // No Apple speech recognition permission needed - whisper.cpp is local
    Ok((mic_granted, true))
}

/// Run whisper on already-16kHz mono f32 samples. Blocking; call inside
/// spawn_blocking. Shared by full and partial transcription.
fn whisper_infer(resampled: &[f32], lang_code: &str) -> Result<String, String> {
    let lock = get_whisper_lock();
    let guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let ctx = guard.as_ref().ok_or("Whisper context not loaded")?;
    let mut state = ctx.create_state()
        .map_err(|e| format!("Whisper state init failed: {}", e))?;
    let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some(lang_code));
    params.set_n_threads(4);
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    state.full(params, resampled)
        .map_err(|e| format!("Whisper inference failed: {}", e))?;
    let mut text = String::new();
    for i in 0..state.full_n_segments() {
        if let Some(segment) = state.get_segment(i) {
            if let Ok(seg) = segment.to_str_lossy() { text.push_str(&seg); }
        }
    }
    Ok(text.trim().to_string())
}

/// Linear-resample mono f32 samples to 16kHz (whisper's rate).
fn resample_16k(mono: &[f32], sample_rate: u32) -> Vec<f32> {
    if sample_rate == 16000 { return mono.to_vec(); }
    let ratio = 16000.0 / sample_rate as f32;
    let new_len = (mono.len() as f32 * ratio) as usize;
    (0..new_len)
        .map(|i| mono.get((i as f32 / ratio) as usize).copied().unwrap_or(0.0))
        .collect()
}

/// Transcribe in-memory mono f32 samples (partial-transcript path). Used
/// by the live loop while the user is still speaking.
pub async fn transcribe_samples(mono: Vec<f32>, sample_rate: u32, language: &str) -> Result<String, String> {
    ensure_whisper_loaded().await?;
    let lang_code = language.split('-').next().unwrap_or("en").to_string();
    tokio::task::spawn_blocking(move || {
        let resampled = resample_16k(&mono, sample_rate);
        whisper_infer(&resampled, &lang_code)
    }).await.map_err(|e| format!("whisper join: {}", e))?
}

pub async fn transcribe_wav(wav_path: &PathBuf, language: &str) -> Result<String, String> {
    // Load whisper if not already loaded
    ensure_whisper_loaded().await?;

    // Read the WAV file and convert to f32 mono samples
    let wav_path_str = wav_path.to_string_lossy().to_string();
    let lang_code = language.split('-').next().unwrap_or("en").to_string();

    let result = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let mut reader = hound::WavReader::open(&wav_path_str)
            .map_err(|e| format!("Failed to open WAV: {}", e))?;
        let spec = reader.spec();

        // Convert samples to f32 mono
        let raw_samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let max_val = 2_i32.pow(spec.bits_per_sample as u32 - 1) as f32;
                reader.samples::<i32>()
                    .filter_map(|s| s.ok())
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
            hound::SampleFormat::Float => {
                reader.samples::<f32>().filter_map(|s| s.ok()).collect()
            }
        };

        // Mix down to mono if needed
        let mono_samples: Vec<f32> = if spec.channels > 1 {
            raw_samples.chunks(spec.channels as usize)
                .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
                .collect()
        } else {
            raw_samples
        };

        // Resample to 16kHz if needed (whisper requires 16kHz)
        let resampled: Vec<f32> = if spec.sample_rate != 16000 {
            let ratio = 16000.0 / spec.sample_rate as f32;
            let new_len = (mono_samples.len() as f32 * ratio) as usize;
            (0..new_len)
                .map(|i| {
                    let src_idx = (i as f32 / ratio) as usize;
                    mono_samples.get(src_idx).copied().unwrap_or(0.0)
                })
                .collect()
        } else {
            mono_samples
        };

        eprintln!("[voice] Whisper transcribing: {} samples ({}s @ 16kHz, lang={})",
            resampled.len(), resampled.len() / 16000, lang_code);

        whisper_infer(&resampled, &lang_code)
    }).await.map_err(|e| format!("Whisper task join failed: {}", e))??;

    if result.is_empty() {
        return Err("No speech detected".to_string());
    }
    Ok(result)
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub async fn transcribe_wav_swift_legacy(wav_path: &PathBuf, language: &str) -> Result<String, String> {
    let swift_code = format!(
        r#"
import Speech
import Foundation

let semaphore = DispatchSemaphore(value: 0)
var resultText = ""
var errorText = ""

SFSpeechRecognizer.requestAuthorization {{ status in
    guard status == .authorized else {{
        errorText = "Speech recognition not authorized (status: \(status.rawValue))"
        semaphore.signal()
        return
    }}

    let locale = Locale(identifier: "{language}")
    guard let recognizer = SFSpeechRecognizer(locale: locale), recognizer.isAvailable else {{
        errorText = "Speech recognizer not available for locale {language}"
        semaphore.signal()
        return
    }}

    let url = URL(fileURLWithPath: "{wav_path}")
    let request = SFSpeechURLRecognitionRequest(url: url)
    request.shouldReportPartialResults = false

    recognizer.recognitionTask(with: request) {{ result, error in
        if let error = error {{
            errorText = error.localizedDescription
            semaphore.signal()
            return
        }}
        if let result = result, result.isFinal {{
            resultText = result.bestTranscription.formattedString
            semaphore.signal()
        }}
    }}
}}

semaphore.wait()

if !errorText.isEmpty {{
    FileHandle.standardError.write(("ERROR: " + errorText + "\n").data(using: .utf8)!)
    exit(1)
}}
print(resultText)
"#,
        language = language,
        wav_path = wav_path.display()
    );

    let output = tokio::process::Command::new("swift")
        .arg("-e")
        .arg(&swift_code)
        .output()
        .await
        .map_err(|e| format!("Failed to run Swift transcription: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Transcription failed: {}", stderr.trim()));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err("No speech detected".to_string());
    }

    Ok(text)
}
