use clap::Parser;
use tonic::transport::Server;

use council_daemon::server::CouncilService;
use council_proto::council_server::CouncilServer;

/// Council deliberation daemon - gRPC server for structured group discussions.
///
/// Starts a persistent gRPC server that manages multiple council sessions.
/// Sessions are created via the CreateSession RPC (use `council-cli create`).
/// The daemon knows nothing about LLMs - any process that can call gRPC can participate.
#[derive(Parser)]
#[command(name = "council-daemon")]
struct Cli {
    /// Port to listen on
    #[arg(long, default_value_t = 50051)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let addr = format!("[::1]:{}", cli.port).parse()?;
    let service = CouncilService::new();

    eprintln!("Council daemon starting on {}", addr);
    eprintln!("Waiting for CreateSession RPCs...");

    Server::builder()
        .add_service(CouncilServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
