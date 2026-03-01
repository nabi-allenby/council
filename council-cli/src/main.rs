mod client;
mod error;

use clap::{Parser, Subcommand};
use council_proto::WaitStatus;

/// Participate in council deliberations.
///
/// A council is a structured group discussion where participants discuss a
/// question across rounds, then cast binding votes. Any process that can
/// run shell commands can participate.
///
/// LIFECYCLE:
///   1. join     - Join the session lobby
///   2. wait     - Poll until it is your turn (long-polls, blocks until ready)
///   3. respond  - Submit your discussion response when it is your turn
///   4. vote     - Cast your binding vote when the vote phase begins
///   5. results  - Retrieve the final decision after voting completes
///
/// TYPICAL WORKFLOW:
///   council-cli join --addr localhost:50051 --name "My Role"
///   # Loop: call wait, then respond when your_turn, repeat each round
///   council-cli wait --addr localhost:50051 --session S1 --name "My Role" --token T1
///   council-cli respond --addr ... --session S1 --name "My Role" --token T1 \
///     --position "..." --reasoning "..." --reasoning "..."
///   # When wait returns vote_phase:
///   council-cli vote --addr ... --session S1 --name "My Role" --token T1 \
///     --choice yay --reason "..."
///   # After all votes:
///   council-cli results --addr ... --session S1
///
/// OUTPUT FORMAT:
///   All output is structured text, one field per line:
///     status: your_turn
///     round: 2
///     transcript: ...
///   Parse the "status" line to determine your next action.
#[derive(Parser)]
#[command(name = "council-cli")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Join a council session lobby.
    ///
    /// Registers you as a participant. Returns the session ID, question,
    /// participant list, and a token you must use for all subsequent commands.
    Join {
        /// Daemon address (host:port)
        #[arg(long, default_value = "[::1]:50051")]
        addr: String,

        /// Your display name or role (must be unique in the session)
        #[arg(long)]
        name: String,
    },

    /// Wait for your turn (long-poll).
    ///
    /// Blocks until one of these statuses is returned:
    ///   your_turn  - It is your turn to respond. Transcript is included.
    ///   vote_phase - Discussion is over. Time to cast your vote.
    ///   complete   - Session is finished. Use 'results' to see the outcome.
    ///   waiting    - Timeout reached; still waiting for your turn.
    ///   lobby      - Still in lobby, waiting for more participants.
    Wait {
        /// Daemon address (host:port)
        #[arg(long, default_value = "[::1]:50051")]
        addr: String,

        /// Session ID (from join output)
        #[arg(long)]
        session: String,

        /// Your participant name (must match join)
        #[arg(long)]
        name: String,

        /// Your participant token (from join output)
        #[arg(long)]
        token: String,

        /// How long to wait in seconds before returning current status
        #[arg(long, default_value_t = 30)]
        timeout: u32,
    },

    /// Submit your discussion response when it is your turn.
    ///
    /// You must wait for 'your_turn' status before calling this.
    /// Provide your position (1 sentence), reasoning (1-5 points),
    /// and optionally concerns (0-5 points).
    Respond {
        /// Daemon address (host:port)
        #[arg(long, default_value = "[::1]:50051")]
        addr: String,

        /// Session ID
        #[arg(long)]
        session: String,

        /// Your participant name
        #[arg(long)]
        name: String,

        /// Your participant token
        #[arg(long)]
        token: String,

        /// Your one-sentence position on the question
        #[arg(long)]
        position: String,

        /// Supporting reasoning points (repeat for multiple: --reasoning "A" --reasoning "B")
        #[arg(long, num_args = 1)]
        reasoning: Vec<String>,

        /// Outstanding concerns (repeat for multiple: --concerns "X" --concerns "Y")
        #[arg(long, num_args = 1)]
        concerns: Vec<String>,
    },

    /// Cast your binding vote when the vote phase begins.
    ///
    /// You must wait for 'vote_phase' status before calling this.
    /// Choose 'yay' to approve or 'nay' to reject the question.
    Vote {
        /// Daemon address (host:port)
        #[arg(long, default_value = "[::1]:50051")]
        addr: String,

        /// Session ID
        #[arg(long)]
        session: String,

        /// Your participant name
        #[arg(long)]
        name: String,

        /// Your participant token
        #[arg(long)]
        token: String,

        /// Your vote: 'yay' or 'nay'
        #[arg(long)]
        choice: String,

        /// 1-2 sentences explaining your vote
        #[arg(long)]
        reason: String,
    },

    /// Retrieve the final decision record after voting completes.
    ///
    /// Returns the outcome (approved/rejected), vote breakdown,
    /// and a full markdown report of the deliberation.
    Results {
        /// Daemon address (host:port)
        #[arg(long, default_value = "[::1]:50051")]
        addr: String,

        /// Session ID
        #[arg(long)]
        session: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Join { addr, name } => run_join(&addr, &name).await,
        Commands::Wait {
            addr,
            session,
            name,
            token,
            timeout,
        } => run_wait(&addr, &session, &name, &token, timeout).await,
        Commands::Respond {
            addr,
            session,
            name,
            token,
            position,
            reasoning,
            concerns,
        } => {
            run_respond(
                &addr, &session, &name, &token, &position, reasoning, concerns,
            )
            .await
        }
        Commands::Vote {
            addr,
            session,
            name,
            token,
            choice,
            reason,
        } => run_vote(&addr, &session, &name, &token, &choice, &reason).await,
        Commands::Results { addr, session } => run_results(&addr, &session).await,
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

async fn run_join(addr: &str, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::join(addr, name).await?;
    println!("session_id: {}", resp.session_id);
    println!("question: {}", resp.question);
    println!("participants: {}", resp.participants.join(", "));
    println!("status: {}", format_session_status(resp.status));
    println!("rounds: {}", resp.rounds);
    println!("min_participants: {}", resp.min_participants);
    println!("participant_token: {}", resp.participant_token);
    Ok(())
}

async fn run_wait(
    addr: &str,
    session: &str,
    name: &str,
    token: &str,
    timeout: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::wait(addr, session, name, token, timeout).await?;
    println!("status: {}", format_wait_status(resp.status));
    println!("round: {}/{}", resp.current_round, resp.total_rounds);
    println!("current_speaker: {}", resp.current_speaker);
    println!("participants: {}", resp.participants.join(", "));
    println!("question: {}", resp.question);
    if !resp.transcript.is_empty() {
        println!("transcript: {}", resp.transcript);
    }
    Ok(())
}

async fn run_respond(
    addr: &str,
    session: &str,
    name: &str,
    token: &str,
    position: &str,
    reasoning: Vec<String>,
    concerns: Vec<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::respond(addr, session, name, token, position, reasoning, concerns).await?;
    println!("accepted: {}", resp.accepted);
    println!("next_step: {}", resp.next_step);
    Ok(())
}

async fn run_vote(
    addr: &str,
    session: &str,
    name: &str,
    token: &str,
    choice: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::vote(addr, session, name, token, choice, reason).await?;
    println!("accepted: {}", resp.accepted);
    println!("message: {}", resp.message);
    Ok(())
}

async fn run_results(addr: &str, session: &str) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::results(addr, session).await?;
    let outcome = match resp.outcome {
        x if x == council_proto::Outcome::Approved as i32 => "APPROVED",
        x if x == council_proto::Outcome::Rejected as i32 => "REJECTED",
        _ => "UNKNOWN",
    };
    println!("outcome: {}", outcome);
    println!("yay_count: {}", resp.yay_count);
    println!("nay_count: {}", resp.nay_count);
    for v in &resp.votes {
        let choice = match v.choice {
            x if x == council_proto::VoteChoice::Yay as i32 => "yay",
            x if x == council_proto::VoteChoice::Nay as i32 => "nay",
            _ => "unknown",
        };
        println!("vote: {} {} \"{}\"", v.participant, choice, v.reason);
    }
    if !resp.decision_record.is_empty() {
        println!("---");
        println!("{}", resp.decision_record);
    }
    Ok(())
}

fn format_wait_status(status: i32) -> &'static str {
    match status {
        x if x == WaitStatus::YourTurn as i32 => "your_turn",
        x if x == WaitStatus::Waiting as i32 => "waiting",
        x if x == WaitStatus::VotePhase as i32 => "vote_phase",
        x if x == WaitStatus::Complete as i32 => "complete",
        x if x == WaitStatus::Lobby as i32 => "lobby",
        _ => "unknown",
    }
}

fn format_session_status(status: i32) -> &'static str {
    match status {
        x if x == council_proto::SessionStatus::LobbyOpen as i32 => "lobby_open",
        x if x == council_proto::SessionStatus::InProgress as i32 => "in_progress",
        x if x == council_proto::SessionStatus::Voting as i32 => "voting",
        x if x == council_proto::SessionStatus::Completed as i32 => "completed",
        _ => "unknown",
    }
}
