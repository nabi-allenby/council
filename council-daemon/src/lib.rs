pub mod config;
pub mod daemon_config;
pub mod error;
pub mod output;
pub mod report;
pub mod server;
pub mod setup;
pub mod types;

use clap::{Parser, Subcommand};
use tonic::transport::Server;

use daemon_config::DaemonConfig;
use server::CouncilService;

/// Council deliberation daemon - gRPC server for structured group discussions.
///
/// Use `setup` for first-time installation: creates config, installs hooks,
/// and starts the daemon in the background. Use `run` to start the server
/// in the foreground (for development or debugging).
#[derive(Parser)]
#[command(name = "council-daemon")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// One-time setup: create config, install hooks, start daemon in background.
    Setup {
        /// Override the default daemon port
        #[arg(long)]
        port: Option<u16>,
    },

    /// Run the gRPC server in the foreground.
    ///
    /// Used internally by `setup` and for development/debugging.
    /// Reads port from config.toml unless --port is specified.
    Run {
        /// Port to listen on (overrides config.toml)
        #[arg(long)]
        port: Option<u16>,
    },

    /// Check if the daemon is running.
    Status,

    /// Gracefully stop the background daemon.
    Stop,

    /// Show daemon log output.
    Logs {
        /// Follow log output (like tail -f)
        #[arg(long, short)]
        follow: bool,

        /// Number of lines to show
        #[arg(long, short, default_value_t = 50)]
        num: usize,
    },
}

/// Daemon entry point. Parses args and dispatches to the appropriate command.
pub async fn daemon_main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Setup { port } => setup::run_setup(port),
        Commands::Run { port } => run_server(port).await,
        Commands::Status => setup::run_status(),
        Commands::Stop => setup::run_stop(),
        Commands::Logs { follow, num } => setup::run_logs(follow, num),
    }
}

async fn run_server(port_override: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let config = DaemonConfig::load();
    let port = port_override.unwrap_or(config.daemon.port);
    let addr = format!("{}:{}", config.daemon.host, port).parse()?;
    let service = CouncilService::new();

    eprintln!("Council daemon starting on {}", addr);
    eprintln!("Waiting for CreateSession RPCs...");

    Server::builder()
        .add_service(council_proto::council_server::CouncilServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
