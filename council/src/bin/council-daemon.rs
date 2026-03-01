#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    council_daemon::daemon_main().await
}
