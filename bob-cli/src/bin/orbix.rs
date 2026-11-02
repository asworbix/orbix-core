// The `orbix` terminal CLI: Orbix Core's local-first agent from the shell.
#[tokio::main]
async fn main() -> std::process::ExitCode {
    orbix_cli::run().await
}
