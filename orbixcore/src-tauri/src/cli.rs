// ─────────────────────────────────────────────
//  OrbixCore CLI
//  Usage:
//    orbixcore                     → launch GUI (default)
//    orbixcore auth login github   → device flow / browser login
//    orbixcore auth login slack    → browser PKCE login
//    orbixcore auth status         → show connected services
//    orbixcore auth logout github  → remove credentials
//    orbixcore auth set-client-id github Ov23liXXXXX
//    orbixcore auth whoami         → quick identity check
// ─────────────────────────────────────────────

use crate::cli_auth;

#[derive(Debug)]
pub enum CliCommand {
    /// No CLI args - launch the normal Tauri GUI
    LaunchGui,
    /// auth login <provider>
    AuthLogin(String),
    /// auth logout <provider>
    AuthLogout(String),
    /// auth status
    AuthStatus,
    /// auth whoami
    AuthWhoami,
    /// auth set-client-id <provider> <client_id>
    AuthSetClientId(String, String),
    /// Show help
    Help,
    /// Unknown command
    Unknown(String),
}

pub fn parse_args() -> CliCommand {
    let args: Vec<String> = std::env::args().collect();

    // No args or first arg starts with -- (Tauri flags) → GUI
    if args.len() < 2 || args[1].starts_with('-') {
        return CliCommand::LaunchGui;
    }

    match args[1].as_str() {
        "auth" => parse_auth_args(&args[2..]),
        "help" | "--help" | "-h" => CliCommand::Help,
        other => CliCommand::Unknown(other.to_string()),
    }
}

fn parse_auth_args(args: &[String]) -> CliCommand {
    if args.is_empty() {
        return CliCommand::AuthStatus;
    }

    match args[0].as_str() {
        "login" => {
            if args.len() < 2 {
                eprintln!("\x1b[31mError:\x1b[0m Missing provider name.");
                eprintln!();
                eprintln!("Usage: orbixcore auth login <provider>");
                eprintln!();
                eprintln!("Providers: github, microsoft, google, slack");
                return CliCommand::Help;
            }
            CliCommand::AuthLogin(args[1].to_lowercase())
        }
        "logout" => {
            if args.len() < 2 {
                eprintln!("\x1b[31mError:\x1b[0m Missing provider name.");
                return CliCommand::Help;
            }
            CliCommand::AuthLogout(args[1].to_lowercase())
        }
        "status" => CliCommand::AuthStatus,
        "whoami" => CliCommand::AuthWhoami,
        "set-client-id" => {
            if args.len() < 3 {
                eprintln!("\x1b[31mError:\x1b[0m Usage: orbixcore auth set-client-id <provider> <client_id>");
                return CliCommand::Help;
            }
            CliCommand::AuthSetClientId(args[1].to_lowercase(), args[2].clone())
        }
        other => {
            eprintln!("\x1b[31mError:\x1b[0m Unknown auth command: {}", other);
            CliCommand::Help
        }
    }
}

pub fn print_help() {
    eprintln!();
    eprintln!("  \x1b[1;97mOrbixCore CLI\x1b[0m");
    eprintln!();
    eprintln!("  \x1b[1mUSAGE:\x1b[0m");
    eprintln!("    orbixcore                                  Launch the app");
    eprintln!("    orbixcore auth login <provider>             Connect a service");
    eprintln!("    orbixcore auth logout <provider>            Disconnect a service");
    eprintln!("    orbixcore auth status                       Show connected services");
    eprintln!("    orbixcore auth whoami                       Show current identity");
    eprintln!("    orbixcore auth set-client-id <p> <id>       Set OAuth client ID");
    eprintln!();
    eprintln!("  \x1b[1mPROVIDERS:\x1b[0m");
    for p in cli_auth::providers() {
        let method = if p.supports_device_flow { "device flow" } else { "browser" };
        eprintln!("    \x1b[36m{:<14}\x1b[0m {} \x1b[90m({})\x1b[0m", p.name, p.display_name, method);
    }
    eprintln!();
    eprintln!("  \x1b[1mEXAMPLES:\x1b[0m");
    eprintln!("    \x1b[90m$\x1b[0m orbixcore auth login github");
    eprintln!("    \x1b[90m$\x1b[0m orbixcore auth login microsoft");
    eprintln!("    \x1b[90m$\x1b[0m orbixcore auth status");
    eprintln!("    \x1b[90m$\x1b[0m orbixcore auth set-client-id github Ov23liXXXXXXXX");
    eprintln!();
}

/// Run the CLI command. Returns true if the command was handled (should exit),
/// false if the GUI should launch.
pub async fn run(cmd: CliCommand) -> bool {
    match cmd {
        CliCommand::LaunchGui => false,

        CliCommand::AuthLogin(provider) => {
            if let Err(e) = cli_auth::login(&provider).await {
                eprintln!("  \x1b[1;31mERR\x1b[0m {}", e);
                std::process::exit(1);
            }
            true
        }

        CliCommand::AuthLogout(provider) => {
            if let Err(e) = cli_auth::logout(&provider).await {
                eprintln!("  \x1b[1;31mERR\x1b[0m {}", e);
                std::process::exit(1);
            }
            true
        }

        CliCommand::AuthStatus => {
            cli_auth::print_status();
            true
        }

        CliCommand::AuthWhoami => {
            let services = cli_auth::load_services();
            if services.is_empty() {
                eprintln!("  \x1b[90mNot logged in to any services.\x1b[0m");
            } else {
                for (provider, service) in &services {
                    let name = service.user_name.as_deref().unwrap_or("?");
                    let email = service.user_email.as_deref().unwrap_or("");
                    let cfg = cli_auth::get_provider(provider);
                    let display_name = cfg.map(|p| p.display_name).unwrap_or(provider.as_str());
                    if email.is_empty() {
                        eprintln!("  \x1b[32m*\x1b[0m \x1b[1m{}\x1b[0m: {}", display_name, name);
                    } else {
                        eprintln!("  \x1b[32m*\x1b[0m \x1b[1m{}\x1b[0m: {} <{}>", display_name, name, email);
                    }
                }
            }
            true
        }

        CliCommand::AuthSetClientId(provider, client_id) => {
            if cli_auth::get_provider(&provider).is_none() {
                eprintln!("  \x1b[1;31mERR\x1b[0m Unknown provider: {}", provider);
                std::process::exit(1);
            }
            let mut ids = crate::oauth::load_oauth_client_ids();
            ids.insert(provider.clone(), client_id);
            match crate::oauth::save_oauth_client_ids(&ids) {
                Ok(()) => {
                    let cfg = cli_auth::get_provider(&provider).unwrap();
                    eprintln!("  \x1b[1;32mOK\x1b[0m Client ID set for \x1b[1m{}\x1b[0m", cfg.display_name);
                    eprintln!("  \x1b[90mNow run: orbixcore auth login {}\x1b[0m", provider);
                }
                Err(e) => {
                    eprintln!("  \x1b[1;31mERR\x1b[0m Failed to save: {}", e);
                    std::process::exit(1);
                }
            }
            true
        }

        CliCommand::Help => {
            print_help();
            true
        }

        CliCommand::Unknown(cmd) => {
            eprintln!("\x1b[31mError:\x1b[0m Unknown command: {}", cmd);
            print_help();
            std::process::exit(1);
        }
    }
}
