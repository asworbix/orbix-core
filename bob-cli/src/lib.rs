// bob - Orbix's local-first agent as a CLI.
//
// There is one Bob: the one in the Orbix Core app. `bob "<task>"` asks
// that Bob from this terminal (see ask.rs), with this folder as the
// turn's working folder, so the terminal gets the app's memory, tools,
// workflows and paired apps. If the app is not running it is started
// headless first. The other commands (chat, connect, handoff, mcp,
// up/down) are thin clients of the same app.
//
// Why a separate crate (not a second [[bin]] in src-tauri): keeps
// the CLI free of Tauri's macOS-only system deps so it cross-compiles
// cleanly and installs as a single static binary.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use anstyle::{AnsiColor, Style};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;

pub mod ask;
pub mod config;
pub mod discovery;
pub mod handoff;
pub mod wordmark;
pub mod mcp;
pub mod stream;

const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8847";
const DEFAULT_MODEL: &str = "gemma-4-e4b";

#[derive(Parser, Debug)]
#[command(name = "orbix", version, about = "orbix - Orbix Core's local-first agent from the terminal")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// What to ask Bob. If omitted (and no subcommand), reads stdin or opens a REPL.
    prompt: Option<Vec<String>>,

    /// Model server for `status` and `models`. Defaults to
    /// ORBIX_ENDPOINT/LLAMA_ENDPOINT env or http://127.0.0.1:8847 (the
    /// Orbix Core app's server).
    #[arg(long, global = true)]
    endpoint: Option<String>,

    /// Model id `orbix chat` sends. Overrides the saved default. Bob itself
    /// uses the model selected in the app.
    #[arg(long, global = true)]
    model: Option<String>,

    /// Print every raw event from the app. Debugging aid.
    #[arg(long, global = true)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// List or select the local model.
    Models {
        #[command(subcommand)]
        action: Option<ModelsAction>,
    },
    /// Show runtime status: app, model server, loaded model.
    Status,
    /// Execute a handoff plan through the Orbix app: `orbix handoff
    /// plan.json` or `cat plan.json | orbix handoff`. Streams run
    /// progress; exit 0 only when the run completes. Spec contract:
    /// docs/orbix-handoff.md.
    Handoff {
        /// HandoffSpec JSON file. Omit to read stdin.
        file: Option<PathBuf>,
        /// Orbix app server. Default http://127.0.0.1:51231
        /// (or ORBIX_APP_URL / saved config).
        #[arg(long)]
        app_url: Option<String>,
        /// Endpoint token from Orbix Settings > Models
        /// (or ORBIX_TOKEN / saved config).
        #[arg(long)]
        token: Option<String>,
        /// Return immediately after the run is accepted.
        #[arg(long)]
        detach: bool,
        /// Save --token / --app-url into the CLI config for next time.
        #[arg(long)]
        save: bool,
    },
    /// Chat with a running Orbix app over the network - point this at the
    /// LAN endpoint from another device. Streams from the app's
    /// /v1/chat/completions. Enter a prompt for one-shot, omit for a REPL.
    Chat {
        /// Prompt to send. Omit for an interactive REPL.
        prompt: Option<Vec<String>>,
        /// Orbix app endpoint, e.g. http://192.168.1.20:51231 (from
        /// Settings > Models > Expose local model endpoint). Or ORBIX_APP_URL
        /// / saved config.
        #[arg(long)]
        endpoint: Option<String>,
        /// Endpoint token from Orbix Settings > Models (or ORBIX_TOKEN /
        /// saved config).
        #[arg(long)]
        token: Option<String>,
        /// Save --endpoint / --token into the CLI config for next time.
        #[arg(long)]
        save: bool,
    },
    /// Start the full Orbix Core app headless: services, workflows, and
    /// the local endpoint with no window and no Dock icon. Foreground by
    /// default - Ctrl-C stops the app and its model servers.
    Up {
        /// Path to Orbix Core.app (or its binary). Default: /Applications.
        #[arg(long)]
        app: Option<PathBuf>,
        /// Leave the app running in the background and return.
        #[arg(long)]
        detach: bool,
    },
    /// Serve Orbix Core to another app as an MCP tool server over stdio.
    /// Pair mode writes this into Claude Desktop / Cursor configs; you can
    /// also add it by hand: {"command": "orbix", "args": ["mcp"]}.
    Mcp {
        /// Orbix app server. Default http://127.0.0.1:51231
        /// (or ORBIX_APP_URL / saved config).
        #[arg(long)]
        app_url: Option<String>,
        /// Name of the app hosting this server, shown in Orbix's chat.
        #[arg(long)]
        client: Option<String>,
    },
    /// Stop a running Orbix Core (headless or windowed) and its model
    /// servers.
    Down,
    /// Find Orbix hosts on your network and chat with one. Discovers devices
    /// running Orbix Core with the endpoint turned on (Settings > Models),
    /// lists them, and connects to your pick. No IP to type.
    Connect {
        /// Seconds to scan the network for hosts (default 3).
        #[arg(long, default_value_t = 3)]
        timeout: u64,
        /// Token for the chosen host (else ORBIX_TOKEN, saved config, or a
        /// prompt). The host advertises that it needs one but never sends it.
        #[arg(long)]
        token: Option<String>,
        /// Save the chosen host + token into the CLI config for next time.
        #[arg(long)]
        save: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ModelsAction {
    /// List downloaded local models (marks the loaded one).
    List,
    /// Set the default model this CLI sends to the server.
    Use { id: String },
}

/// Shared entry point for both the `orbix` and `bob` binaries.
pub async fn run() -> std::process::ExitCode {
    let cli = Cli::parse();
    let endpoint = cli.endpoint.clone()
        .or_else(|| std::env::var("ORBIX_ENDPOINT").ok())
        .or_else(|| std::env::var("LLAMA_ENDPOINT").ok())
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    let mut cfg = config::CliConfig::load();
    let model = cli.model.clone().unwrap_or_else(|| {
        if cfg.model.is_empty() { DEFAULT_MODEL.to_string() } else { cfg.model.clone() }
    });

    // ── Subcommands ──
    if let Some(cmd) = cli.command {
        return match cmd {
            Command::Status => { cmd_status(&endpoint, &model).await; std::process::ExitCode::SUCCESS }
            Command::Models { action } => cmd_models(&endpoint, &model, action).await,
            Command::Handoff { file, app_url, token, detach, save } => {
                handoff::cmd_handoff(
                    handoff::HandoffOpts { file, app_url, token, detach, save },
                    &mut cfg,
                )
                .await
            }
            Command::Up { app, detach } => cmd_up(app, detach).await,
            Command::Down => cmd_down().await,
            Command::Mcp { app_url, client } => mcp::cmd_mcp(mcp::McpOpts { app_url, client }, &cfg).await,
            Command::Chat { prompt, endpoint, token, save } => {
                cmd_chat(prompt, endpoint, token, save, &model, cli.verbose, &mut cfg).await
            }
            Command::Connect { timeout, token, save } => {
                cmd_connect(timeout, token, save, &model, cli.verbose, &mut cfg).await
            }
        };
    }

    // ── Default: chat / agentic ──
    let initial_prompt = match cli.prompt {
        Some(words) if !words.is_empty() => words.join(" "),
        _ => {
            if !std::io::stdin().is_terminal() {
                let mut s = String::new();
                if std::io::stdin().read_line(&mut s).is_err() {
                    return std::process::ExitCode::from(1);
                }
                s.trim().to_string()
            } else {
                return repl(cli.verbose).await;
            }
        }
    };
    if initial_prompt.is_empty() {
        eprintln!("orbix: empty prompt; nothing to do");
        return std::process::ExitCode::from(1);
    }

    if let Err(e) = ask::ensure_app().await {
        eprintln!("orbix: {}", e);
        return std::process::ExitCode::from(2);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    match ask::ask(&initial_prompt, &cwd, cli.verbose).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("orbix: {}", e);
            std::process::ExitCode::from(1)
        }
    }
}

/// `orbix status`: is the app up, is the model server up, which model is
/// loaded. Reports only; starts nothing.
async fn cmd_status(endpoint: &str, model: &str) {
    orbix_banner();
    let app_up = ask::app_is_up().await;
    let reachable = server_answers(endpoint).await;
    let loaded = if reachable { loaded_model(endpoint).await } else { None };
    print_kv("App", if app_up { "\u{2713} running (Bob answers `bob \"...\"`)" } else { "\u{2717} not running (`bob` starts it)" });
    print_kv("Model server", &format!("{} {}", if reachable { "\u{2713}" } else { "\u{2717}" }, endpoint));
    print_kv("Loaded model", loaded.as_deref().unwrap_or("(none loaded)"));
    print_kv("Chat model", model);
}

/// `orbix models [list|use]`.
async fn cmd_models(endpoint: &str, model: &str, action: Option<ModelsAction>) -> std::process::ExitCode {
    match action.unwrap_or(ModelsAction::List) {
        ModelsAction::List => {
            let loaded = loaded_model(endpoint).await;
            let downloaded = config::downloaded_models();
            if downloaded.is_empty() {
                println!("No downloaded models found in {}", config::models_dir().display());
                println!("Open Orbix Core to download one, or add a model there.");
            } else {
                println!("Local models ({}):", config::models_dir().display());
                for m in &downloaded {
                    let mark = if loaded.as_deref() == Some(m.as_str()) { " \u{2190} loaded" } else { "" };
                    let sel = if m == model { " *" } else { "  " };
                    println!("{}{}{}", sel, m, mark);
                }
                println!("\n* = this CLI's default. `orbix models use <id>` to change.");
            }
            std::process::ExitCode::SUCCESS
        }
        ModelsAction::Use { id } => {
            let mut cfg = config::CliConfig::load();
            cfg.model = id.clone();
            match cfg.save() {
                Ok(()) => { println!("Default model set to {}", id); std::process::ExitCode::SUCCESS }
                Err(e) => { eprintln!("orbix: {}", e); std::process::ExitCode::from(1) }
            }
        }
    }
}

/// Query the server for the currently-loaded model id via /v1/models.
async fn loaded_model(endpoint: &str) -> Option<String> {
    let client = reqwest::Client::builder().timeout(Duration::from_millis(1200)).build().ok()?;
    let v: serde_json::Value = client.get(format!("{}/v1/models", endpoint)).send().await.ok()?.json().await.ok()?;
    v.get("data")?.as_array()?.first()?.get("id")?.as_str().map(|s| s.to_string())
}

pub(crate) fn orbix_banner() {
    let mut out = anstream::stdout().lock();
    let style = Style::new().fg_color(Some(AnsiColor::BrightMagenta.into())).bold();
    let _ = writeln!(out, "{}orbix{} \u{25c9} Orbix Core", style.render(), style.render_reset());
}

pub(crate) fn print_kv(k: &str, v: &str) {
    let mut out = anstream::stdout().lock();
    let dim = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
    let _ = writeln!(out, "  {}{:<14}{} {}", dim.render(), k, dim.render_reset(), v);
}

/// True when the model server answers its health check.
async fn server_answers(endpoint: &str) -> bool {
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_millis(800)).build() else {
        return false;
    };
    matches!(client.get(format!("{}/health", endpoint)).send().await, Ok(r) if r.status().is_success())
}

/// `orbix up`: the whole app from a terminal - no window, no Dock icon.
async fn cmd_up(app: Option<PathBuf>, detach: bool) -> std::process::ExitCode {
    let warm_url = "http://127.0.0.1:51231/cli/warm";
    let quick = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("http client");
    if matches!(quick.get(warm_url).send().await, Ok(r) if r.status().is_success()) {
        eprintln!("orbix: Orbix Core already answers on 51231 - nothing to start.");
        print_up_banner();
        return std::process::ExitCode::SUCCESS;
    }

    let bin = match resolve_app_binary(app) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("orbix: {}", e);
            return std::process::ExitCode::from(1);
        }
    };
    eprintln!("orbix: starting Orbix Core headless…");
    let mut cmd = std::process::Command::new(&bin);
    cmd.arg("--headless");
    if detach {
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("orbix: failed to start {}: {}", bin.display(), e);
            return std::process::ExitCode::from(1);
        }
    };

    let mut alive = false;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if matches!(quick.get(warm_url).send().await, Ok(r) if r.status().is_success()) {
            alive = true;
            break;
        }
    }
    if !alive {
        eprintln!("orbix: the app server did not come up on 51231 after 30s.");
        let _ = child.kill();
        return std::process::ExitCode::from(2);
    }

    // Warm the model now so the first chat is instant.
    eprintln!("orbix: loading the model (first start can take a minute)…");
    let patient = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .expect("http client");
    let _ = patient.get(warm_url).send().await;
    print_up_banner();

    if detach {
        eprintln!("orbix: detached - `orbix down` stops it.");
        drop(child);
        return std::process::ExitCode::SUCCESS;
    }
    eprintln!("orbix: foreground - Ctrl-C stops the app and its model servers.");
    let status = tokio::task::spawn_blocking(move || child.wait()).await;
    match status {
        Ok(Ok(s)) if s.success() => std::process::ExitCode::SUCCESS,
        _ => std::process::ExitCode::from(1),
    }
}

/// `orbix down`: stop the app (headless or windowed) and its model servers.
async fn cmd_down() -> std::process::ExitCode {
    #[cfg(windows)]
    {
        // taskkill by image name; /T sweeps the llama-server children too.
        let stopped = std::process::Command::new("taskkill.exe")
            .args(["/IM", "orbixcore.exe", "/T", "/F"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let stopped = std::process::Command::new("taskkill.exe")
            .args(["/IM", "Orbix Core.exe", "/T", "/F"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
            || stopped;
        if !stopped {
            eprintln!("orbix: nothing to stop - no Orbix Core process found.");
            return std::process::ExitCode::from(1);
        }
        // Sweep any stray model server that outlived its parent.
        std::process::Command::new("taskkill.exe")
            .args(["/IM", "llama-server.exe", "/F"])
            .output()
            .ok();
        eprintln!("orbix: stopped Orbix Core and its model servers.");
        std::process::ExitCode::SUCCESS
    }
    #[cfg(not(windows))]
    {
        // SIGTERM first: the headless signal handler cleans up llama children.
        let stopped = std::process::Command::new("pkill")
            .args(["-TERM", "-x", "orbixcore"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !stopped {
            eprintln!("orbix: nothing to stop - no orbixcore process found.");
            return std::process::ExitCode::from(1);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        // The windowed app has no SIGTERM cleanup path - sweep stray servers.
        std::process::Command::new("pkill")
            .args(["-f", "llama-server"])
            .output()
            .ok();
        eprintln!("orbix: stopped Orbix Core and its model servers.");
        std::process::ExitCode::SUCCESS
    }
}

fn resolve_app_binary(app: Option<PathBuf>) -> Result<PathBuf, String> {
    const IN_BUNDLE: &str = "Contents/MacOS/orbixcore";
    let candidates: Vec<PathBuf> = match app {
        Some(p) => {
            let p = if p.extension().map(|e| e == "app").unwrap_or(false) {
                p.join(IN_BUNDLE)
            } else {
                p
            };
            vec![p]
        }
        None if cfg!(windows) => {
            // NSIS installs per-machine under Program Files (binary name is
            // orbixcore.exe, verified against a real install); the per-user
            // installer variant lands under LOCALAPPDATA\Programs.
            let mut v = Vec::new();
            for base in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(pf) = std::env::var(base) {
                    let dir = PathBuf::from(pf).join("Orbix Core");
                    v.push(dir.join("orbixcore.exe"));
                    v.push(dir.join("Orbix Core.exe"));
                }
            }
            if let Ok(lad) = std::env::var("LOCALAPPDATA") {
                let dir = PathBuf::from(lad).join("Programs").join("Orbix Core");
                v.push(dir.join("orbixcore.exe"));
                v.push(dir.join("Orbix Core.exe"));
            }
            v
        }
        None => {
            let mut v = vec![PathBuf::from("/Applications/Orbix Core.app").join(IN_BUNDLE)];
            if let Ok(home) = std::env::var("HOME") {
                v.push(PathBuf::from(home).join("Applications/Orbix Core.app").join(IN_BUNDLE));
            }
            v
        }
    };
    candidates
        .into_iter()
        .find(|p| p.exists())
        .ok_or_else(|| {
            if cfg!(windows) {
                "Orbix Core is not installed - run the installer, or pass --app <path to Orbix Core.exe>".to_string()
            } else {
                "Orbix Core.app not found - install it, or pass --app /path/to/Orbix Core.app".to_string()
            }
        })
}

fn print_up_banner() {
    print_kv("App server", "http://127.0.0.1:51231  (/v1 + /cli + /handoff)");
    print_kv("Model server", "http://127.0.0.1:8847");
    print_kv("Next", "orbix status · orbix chat · orbix handoff plan.json");
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Msg {
    role: String,
    content: String,
}

async fn repl(verbose: bool) -> std::process::ExitCode {
    // First contact: no config file yet means nobody has used the CLI on
    // this machine. One warm welcome, never repeated.
    let first_run = !config::exists();
    if let Err(e) = ask::ensure_app().await {
        eprintln!("orbix: {}", e);
        return std::process::ExitCode::from(2);
    }
    if first_run {
        wordmark::print_wordmark();
        println!();
        print_dim("  Bob runs on this machine. Local first, no cloud account needed.\n\n");
    } else {
        orbix_banner();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    print_dim(&format!("Bob in {} · same Bob as the app · ctrl-d to exit\n", cwd.display()));
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();
    loop {
        print!("\u{2192} ");
        std::io::stdout().flush().ok();
        // Ctrl-C at the prompt leaves; during a turn it stops the turn.
        let line = tokio::select! {
            l = reader.next_line() => l,
            _ = tokio::signal::ctrl_c() => { println!(); break; }
        };
        let line = match line {
            Ok(Some(l)) => l,
            _ => { println!(); break; }
        };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }
        if trimmed == ":q" || trimmed == "exit" { break; }
        if let Err(e) = ask::ask(trimmed, &cwd, verbose).await {
            eprintln!("orbix: {}", e);
        }
    }
    std::process::ExitCode::SUCCESS
}

pub(crate) fn print_dim(s: &str) {
    let mut out = anstream::stdout().lock();
    let style = Style::new().fg_color(Some(AnsiColor::BrightBlack.into()));
    let _ = write!(out, "{}{}{}", style.render(), s, style.render_reset());
    let _ = out.flush();
}

/// `orbix chat` - talk to a running Orbix app over the network (e.g. from a
/// phone or another laptop pointed at the Mac's LAN endpoint). A plain
/// multi-turn chat streamed from the app's OpenAI-compatible endpoint; the
/// bearer token authenticates each request.
async fn cmd_chat(
    prompt: Option<Vec<String>>,
    endpoint: Option<String>,
    token: Option<String>,
    save: bool,
    model: &str,
    verbose: bool,
    cfg: &mut config::CliConfig,
) -> std::process::ExitCode {
    let endpoint = endpoint
        .or_else(|| std::env::var("ORBIX_APP_URL").ok())
        .or_else(|| if cfg.app_url.is_empty() { None } else { Some(cfg.app_url.clone()) });
    let Some(endpoint) = endpoint else {
        eprintln!("orbix chat: no endpoint.");
        eprintln!("  On the host device: Settings > Models > Expose local model endpoint, then:");
        eprintln!("    orbix chat --endpoint http://<mac-lan-ip>:51231 --token <token> --save");
        return std::process::ExitCode::from(1);
    };
    let token = token
        .or_else(|| std::env::var("ORBIX_TOKEN").ok())
        .or_else(|| if cfg.app_token.is_empty() { None } else { Some(cfg.app_token.clone()) });
    let Some(token) = token else {
        eprintln!("orbix chat: no token. Pass --token <token> (from Settings > Models).");
        return std::process::ExitCode::from(1);
    };

    if save {
        cfg.app_url = endpoint.clone();
        cfg.app_token = token.clone();
        if let Err(e) = cfg.save() { eprintln!("orbix chat: could not save config: {}", e); }
    }

    if let Some(words) = prompt {
        let text = words.join(" ");
        if text.trim().is_empty() { return std::process::ExitCode::from(1); }
        let mut history: Vec<Msg> = vec![chat_system_msg()];
        let ok = chat_turn(&mut history, &endpoint, model, verbose, &token, text).await;
        return if ok { std::process::ExitCode::SUCCESS } else { std::process::ExitCode::from(1) };
    }

    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();
    chat_repl(&mut reader, endpoint, token, model, verbose, cfg).await
}

/// `orbix connect` - find Orbix hosts on the LAN, pick one, and chat with it.
/// Discovery lists what's advertising `_orbix._tcp`; the token still comes
/// from you (env, saved config, or a prompt) because the host never sends it.
async fn cmd_connect(
    timeout_secs: u64,
    token: Option<String>,
    save: bool,
    model: &str,
    verbose: bool,
    cfg: &mut config::CliConfig,
) -> std::process::ExitCode {
    let stdin = tokio::io::stdin();
    let mut reader = tokio::io::BufReader::new(stdin).lines();

    let Some((endpoint, token)) = select_host(&mut reader, timeout_secs, token, cfg).await else {
        return std::process::ExitCode::from(1);
    };

    if save {
        cfg.app_url = endpoint.clone();
        cfg.app_token = token.clone();
        if let Err(e) = cfg.save() { eprintln!("orbix connect: could not save config: {}", e); }
    }

    chat_repl(&mut reader, endpoint, token, model, verbose, cfg).await
}

fn chat_system_msg() -> Msg {
    Msg {
        role: "system".into(),
        content: "You are Bob, Orbix Core's assistant. Answer concisely.".into(),
    }
}

/// The interactive chat loop against a remote host. `/connect` re-discovers
/// and switches host (clearing history); `:q`/`exit` or ctrl-d leaves.
async fn chat_repl(
    reader: &mut tokio::io::Lines<tokio::io::BufReader<tokio::io::Stdin>>,
    mut endpoint: String,
    mut token: String,
    model: &str,
    verbose: bool,
    cfg: &config::CliConfig,
) -> std::process::ExitCode {
    orbix_banner();
    print_dim(&format!("chat · {} · /connect to switch host · ctrl-d to exit\n", endpoint));
    let mut history: Vec<Msg> = vec![chat_system_msg()];
    loop {
        print!("\u{2192} ");
        std::io::stdout().flush().ok();
        let line = match reader.next_line().await {
            Ok(Some(l)) => l,
            _ => { println!(); break; }
        };
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() { continue; }
        if trimmed == ":q" || trimmed == "exit" || trimmed == "/quit" { break; }
        if trimmed == "/connect" || trimmed == "/hosts" {
            if let Some((ep, tok)) = select_host(reader, 3, None, cfg).await {
                endpoint = ep;
                token = tok;
                history = vec![chat_system_msg()];
                print_dim(&format!("connected · {}\n", endpoint));
            }
            continue;
        }
        chat_turn(&mut history, &endpoint, model, verbose, &token, trimmed).await;
    }
    std::process::ExitCode::SUCCESS
}

/// Discover hosts, print a numbered menu, read the user's pick, and resolve a
/// token (arg > ORBIX_TOKEN > saved-for-this-host > prompt). Returns the
/// chosen (endpoint, token), or None if nothing was found or the user bailed.
async fn select_host(
    reader: &mut tokio::io::Lines<tokio::io::BufReader<tokio::io::Stdin>>,
    timeout_secs: u64,
    token_arg: Option<String>,
    cfg: &config::CliConfig,
) -> Option<(String, String)> {
    print_dim(&format!("Scanning the network for Orbix hosts ({}s)…\n", timeout_secs));
    let window = std::time::Duration::from_secs(timeout_secs.max(1));
    let hosts = match tokio::task::spawn_blocking(move || discovery::discover(window)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => { eprintln!("orbix connect: {}", e); return None; }
        Err(e) => { eprintln!("orbix connect: discovery failed to run: {}", e); return None; }
    };
    if hosts.is_empty() {
        eprintln!("No Orbix hosts found on this network.");
        eprintln!("  On the host device: Settings > Models > turn on 'Expose local model endpoint'.");
        eprintln!("  Then make sure both devices are on the same Wi-Fi / LAN.");
        return None;
    }

    println!("Found {} host{}:", hosts.len(), if hosts.len() == 1 { "" } else { "s" });
    for (i, h) in hosts.iter().enumerate() {
        let model = if h.model.is_empty() || h.model == "-" { "no model loaded".to_string() } else { h.model.clone() };
        let os = if h.os.is_empty() { "?".to_string() } else { h.os.clone() };
        println!("  {}) {}  ({} · {})  {}", i + 1, h.name, os, model, h.endpoint());
    }

    let idx = if hosts.len() == 1 {
        0
    } else {
        print!("Pick a host [1-{}]: ", hosts.len());
        std::io::stdout().flush().ok();
        let line = match reader.next_line().await { Ok(Some(l)) => l, _ => return None };
        match line.trim().parse::<usize>() {
            Ok(n) if n >= 1 && n <= hosts.len() => n - 1,
            _ => { eprintln!("orbix connect: not a valid choice."); return None; }
        }
    };
    let host = &hosts[idx];
    let endpoint = host.endpoint();

    let token = token_arg
        .or_else(|| std::env::var("ORBIX_TOKEN").ok())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| {
            if !cfg.app_token.is_empty() && cfg.app_url == endpoint {
                Some(cfg.app_token.clone())
            } else {
                None
            }
        });
    let token = match token {
        Some(t) => t,
        None => {
            print!("Token for {} (from that device's Settings > Models): ", host.name);
            std::io::stdout().flush().ok();
            match reader.next_line().await { Ok(Some(l)) => l.trim().to_string(), _ => return None }
        }
    };
    if token.is_empty() {
        eprintln!("orbix connect: no token given, cancelled.");
        return None;
    }
    Some((endpoint, token))
}

/// One request/response against a remote Orbix app's authenticated endpoint,
/// appending the exchange to `history`. Returns false on a network/auth error
/// (the failed user turn is rolled back so the history stays clean).
async fn chat_turn(
    history: &mut Vec<Msg>,
    endpoint: &str,
    model: &str,
    verbose: bool,
    token: &str,
    text: String,
) -> bool {
    history.push(Msg { role: "user".into(), content: text });
    match stream::stream_chat_auth(endpoint, model, history, verbose, Some(token)).await {
        Ok(reply) => { history.push(Msg { role: "assistant".into(), content: reply }); true }
        Err(e) => {
            eprintln!("orbix chat: {}", e);
            if e.contains("401") { eprintln!("  Check the token against Settings > Models."); }
            if e.contains("error sending request") { eprintln!("  Is the host device reachable and the endpoint turned on?"); }
            history.pop();
            false
        }
    }
}
