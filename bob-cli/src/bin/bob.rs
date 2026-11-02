// `bob` is a back-compat alias for `orbix` (same binary logic).
#[tokio::main]
async fn main() -> std::process::ExitCode {
    orbix_cli::run().await
}
