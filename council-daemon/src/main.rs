use std::time::Duration;

use clap::Parser;
use tonic::transport::Server;

use council_daemon::config::DaemonConfig;
use council_daemon::server::CouncilService;
use council_proto::council_server::CouncilServer;

/// Council deliberation daemon - gRPC server for structured group discussions.
///
/// Starts a gRPC server that manages a single council session. Participants
/// join via the council-cli tool, discuss across rounds, then cast binding votes.
/// The daemon knows nothing about LLMs - any process that can call gRPC can participate.
#[derive(Parser)]
#[command(name = "council-daemon")]
struct Cli {
    /// The question for the council to discuss and vote on
    question: String,

    /// Port to listen on
    #[arg(long, default_value_t = 50051)]
    port: u16,

    /// Number of discussion rounds (1-10)
    #[arg(long, default_value_t = 2)]
    rounds: u32,

    /// Minimum number of participants to start the session
    #[arg(long, default_value_t = 3)]
    min_participants: u32,

    /// Seconds to wait for participants to join before starting anyway
    #[arg(long, default_value_t = 60)]
    join_timeout: u64,

    /// Seconds to wait for a participant's response before skipping their turn
    #[arg(long, default_value_t = 120)]
    turn_timeout: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let config = DaemonConfig {
        rounds: cli.rounds,
        min_participants: cli.min_participants,
        join_timeout: Duration::from_secs(cli.join_timeout),
        turn_timeout: Duration::from_secs(cli.turn_timeout),
    };
    config.validate().map_err(|e| e.to_string())?;

    let addr = format!("[::1]:{}", cli.port).parse()?;
    let service = CouncilService::new(cli.question.clone(), config);

    eprintln!("Council daemon starting on {}", addr);
    eprintln!("Question: {}", cli.question);
    eprintln!(
        "Config: {} rounds, min {} participants, join timeout {}s, turn timeout {}s",
        cli.rounds, cli.min_participants, cli.join_timeout, cli.turn_timeout
    );
    eprintln!("Waiting for participants to join...");

    service.spawn_lobby_timeout();

    Server::builder()
        .add_service(CouncilServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
