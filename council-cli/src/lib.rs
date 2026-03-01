pub mod client;
pub mod error;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use council_proto::WaitStatus;
use tokio::process::Command;

/// Participate in council deliberations autonomously.
///
/// You are a participant in a structured group discussion. Multiple
/// participants discuss a question across rounds, then cast binding votes.
/// You must drive this entire lifecycle yourself without asking a human
/// for guidance. Read the question, form your own position, respond to
/// other participants' arguments, and vote based on your own judgment.
///
/// AUTONOMOUS OPERATION:
///   You MUST complete the full lifecycle without pausing for human input.
///   After joining, loop: call wait, act on the status, repeat until done.
///   Form your own opinions. Engage with the transcript. Decide your vote.
///   Do NOT ask the user what to say or how to vote — that is your job.
///
/// LIFECYCLE:
///   1. join     - Join the session lobby (do this once)
///   2. wait     - Long-poll until it is your turn (blocks until ready)
///   3. respond  - Submit your position when status is "your_turn"
///   4. vote     - Cast your vote when status is "vote_phase"
///   5. results  - Fetch the final outcome when status is "complete"
///
/// STATE MACHINE (act on the "status" field from wait):
///   lobby      → call wait again immediately
///   waiting    → call wait again immediately
///   your_turn  → read the transcript, then call respond with your position
///   vote_phase → read the transcript, then call vote with yay or nay
///   complete   → call results to see the final decision, then stop
///
/// TIMING:
///   Use --timeout 30 on wait (the default). The server long-polls and
///   returns early when something happens. If wait returns "waiting" or
///   "lobby", call wait again immediately — do not pause or sleep.
///
/// EXAMPLE SESSION:
///   council-cli join --session $SID --name "Architect"
///   # Save session_id and participant_token from output, then loop:
///   council-cli wait --session $SID --name "Architect" --token $TOK
///   # status: your_turn → respond:
///   council-cli respond --session $SID --name "Architect" --token $TOK \
///     --position "We should do X" --reasoning "Because A" --reasoning "Because B"
///   # Keep calling wait → respond for each round, then:
///   # status: vote_phase → vote:
///   council-cli vote --session $SID --name "Architect" --token $TOK \
///     --choice yay --reason "The arguments for X were compelling"
///   # status: complete → results:
///   council-cli results --session $SID
///
/// OUTPUT FORMAT:
///   All output is structured text, one field per line:
///     status: your_turn
///     round: 2/3
///     transcript: ...
///   Parse the "status" line to determine your next action.
#[derive(Parser)]
#[command(name = "council-cli")]
struct Cli {
    /// Daemon address (host:port). Auto-detected from
    /// ~/.config/council/config.toml if not specified.
    #[arg(long, global = true)]
    addr: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new council session and optionally spawn participant hooks.
    ///
    /// Creates a session on the daemon, then for each participant name spawns
    /// the --hook script with COUNCIL_SESSION_ID, COUNCIL_PARTICIPANT_NAME,
    /// and COUNCIL_ADDR as environment variables. Use --follow to poll status
    /// and print results when the session completes.
    Create {
        /// The question for the council to discuss and vote on
        #[arg(long)]
        question: String,

        /// Comma-separated participant names
        #[arg(long, value_delimiter = ',')]
        participants: Vec<String>,

        /// Path to hook script to spawn per participant
        #[arg(long)]
        hook: PathBuf,

        /// Number of discussion rounds (1-10)
        #[arg(long, default_value_t = 2)]
        rounds: u32,

        /// Minimum number of participants to start (defaults to participant count)
        #[arg(long)]
        min_participants: Option<u32>,

        /// Seconds to wait for participants to join before starting anyway
        #[arg(long, default_value_t = 60)]
        join_timeout: u32,

        /// Seconds to wait for a participant's response before skipping their turn
        #[arg(long, default_value_t = 120)]
        turn_timeout: u32,

        /// Poll session status and print results when complete
        #[arg(long)]
        follow: bool,
    },

    /// List all sessions on a daemon.
    List,

    /// Get the status of a specific session.
    Status {
        /// Session ID
        #[arg(long)]
        session: String,
    },

    /// Join a council session lobby. Do this once at the start.
    ///
    /// Returns session_id and participant_token — save both, you need them
    /// for every subsequent command. After joining, immediately call wait
    /// to begin the participation loop.
    Join {
        /// Session ID to join
        #[arg(long)]
        session: String,

        /// Your display name or role (must be unique in the session)
        #[arg(long)]
        name: String,
    },

    /// Wait for your turn (long-poll). Call this in a loop.
    ///
    /// Use --timeout 30 (default). The server holds the connection open and
    /// returns as soon as something changes. If it times out with status
    /// "waiting" or "lobby", just call wait again immediately — this is normal.
    ///
    /// Act on the status field as soon as it returns:
    ///   your_turn  → call respond with your position (read transcript first)
    ///   vote_phase → call vote with yay/nay (read transcript first)
    ///   complete   → call results, then stop
    ///   waiting    → call wait again immediately (not your turn yet)
    ///   lobby      → call wait again immediately (waiting for participants)
    Wait {
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

    /// Submit your discussion response. Only call when status is "your_turn".
    ///
    /// Read the transcript from the wait output to understand what others said.
    /// Form your own position — do NOT ask a human what to say. Engage with
    /// the other participants' arguments. After responding, call wait again.
    Respond {
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

    /// Cast your binding vote. Only call when status is "vote_phase".
    ///
    /// Review the full transcript and decide for yourself. Vote yay to approve
    /// or nay to reject. Provide a reason explaining your decision.
    /// Do NOT ask a human how to vote — use your own judgment.
    /// After voting, call wait again to check for completion.
    Vote {
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

    /// Retrieve the final decision record. Only call when status is "complete".
    ///
    /// Returns the outcome (approved/rejected), vote breakdown,
    /// and a full markdown report. This is the last step — you are done.
    Results {
        /// Session ID
        #[arg(long)]
        session: String,
    },
}

/// Resolve the daemon address: CLI flag > config file > default.
fn resolve_addr(addr: Option<String>) -> String {
    if let Some(a) = addr {
        return a;
    }

    // Try reading from config file
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("council").join("config.toml");
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(config) = toml::from_str::<CouncilConfig>(&content) {
                return format!("{}:{}", config.daemon.host, config.daemon.port);
            }
        }
    }

    "[::1]:50051".to_string()
}

/// Minimal config struct for reading daemon address.
#[derive(serde::Deserialize)]
struct CouncilConfig {
    #[serde(default)]
    daemon: DaemonAddr,
}

#[derive(serde::Deserialize)]
struct DaemonAddr {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
}

impl Default for DaemonAddr {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_host() -> String {
    "[::1]".to_string()
}
fn default_port() -> u16 {
    50051
}

/// CLI entry point. Parses args and dispatches to the appropriate command.
pub async fn cli_main() {
    let cli = Cli::parse();
    let addr = resolve_addr(cli.addr);

    let result = match cli.command {
        Commands::Create {
            question,
            participants,
            hook,
            rounds,
            min_participants,
            join_timeout,
            turn_timeout,
            follow,
        } => {
            run_create(
                &addr,
                &question,
                participants,
                hook,
                rounds,
                min_participants,
                join_timeout,
                turn_timeout,
                follow,
            )
            .await
        }
        Commands::List => run_list(&addr).await,
        Commands::Status { session } => run_status(&addr, &session).await,
        Commands::Join { session, name } => run_join(&addr, &session, &name).await,
        Commands::Wait {
            session,
            name,
            token,
            timeout,
        } => run_wait(&addr, &session, &name, &token, timeout).await,
        Commands::Respond {
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
            session,
            name,
            token,
            choice,
            reason,
        } => run_vote(&addr, &session, &name, &token, &choice, &reason).await,
        Commands::Results { session } => run_results(&addr, &session).await,
    };

    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_create(
    addr: &str,
    question: &str,
    participants: Vec<String>,
    hook: PathBuf,
    rounds: u32,
    min_participants: Option<u32>,
    join_timeout: u32,
    turn_timeout: u32,
    follow: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate hook exists before calling the daemon
    if !hook.exists() {
        return Err(format!("hook not found: {}", hook.display()).into());
    }

    let min_p = min_participants.unwrap_or(participants.len() as u32);

    // Reuse a single gRPC client for all RPCs in this flow
    let mut rpc = client::connect(addr).await?;

    let resp = rpc
        .create_session(council_proto::CreateSessionRequest {
            question: question.to_string(),
            rounds,
            min_participants: min_p,
            join_timeout_seconds: join_timeout,
            turn_timeout_seconds: turn_timeout,
        })
        .await?
        .into_inner();
    let session_id = resp.session_id;
    eprintln!("session_id: {}", session_id);

    // Spawn hook per participant
    let mut children = Vec::new();
    for name in &participants {
        eprintln!("Spawning hook for participant: {}", name);
        let child = Command::new(&hook)
            .env("COUNCIL_SESSION_ID", &session_id)
            .env("COUNCIL_PARTICIPANT_NAME", name)
            .env("COUNCIL_ADDR", addr)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("failed to spawn hook for {}: {}", name, e))?;
        children.push((name.clone(), child));
    }

    let result = if follow {
        follow_session(&mut rpc, &session_id).await
    } else {
        Ok(())
    };

    // Always wait for hook processes, even on error
    for (name, mut child) in children {
        match child.wait().await {
            Ok(status) if !status.success() => {
                eprintln!("Warning: hook for {} exited with {}", name, status);
            }
            Err(e) => {
                eprintln!("Warning: failed to wait for hook for {}: {}", name, e);
            }
            _ => {}
        }
    }

    result
}

async fn follow_session(
    rpc: &mut council_proto::council_client::CouncilClient<tonic::transport::Channel>,
    session_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("Following session {}...", session_id);
    let mut consecutive_errors = 0u32;
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let status = match rpc
            .get_session(council_proto::GetSessionRequest {
                session_id: session_id.to_string(),
            })
            .await
        {
            Ok(resp) => {
                consecutive_errors = 0;
                resp.into_inner()
            }
            Err(e) => {
                consecutive_errors += 1;
                if consecutive_errors >= 5 {
                    return Err(format!("lost connection to daemon after 5 retries: {}", e).into());
                }
                eprintln!(
                    "Warning: poll failed (attempt {}/5): {}",
                    consecutive_errors, e
                );
                continue;
            }
        };

        let status_str = format_session_status(status.status);
        eprintln!(
            "[{}] status={} round={}/{} participants={}",
            session_id,
            status_str,
            status.current_round,
            status.total_rounds,
            status.participants.join(", ")
        );

        if status.status == council_proto::SessionStatus::Completed as i32 {
            let results = rpc
                .results(council_proto::ResultsRequest {
                    session_id: session_id.to_string(),
                })
                .await?
                .into_inner();
            print_results(&results);
            return Ok(());
        }
    }
}

async fn run_list(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::list_sessions(addr).await?;
    if resp.sessions.is_empty() {
        println!("No sessions.");
        return Ok(());
    }
    for s in &resp.sessions {
        println!(
            "{} status={} participants={} question=\"{}\"",
            s.session_id,
            format_session_status(s.status),
            s.participant_count,
            s.question
        );
    }
    Ok(())
}

async fn run_status(addr: &str, session_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::get_session(addr, session_id).await?;
    println!("session_id: {}", resp.session_id);
    println!("question: {}", resp.question);
    println!("status: {}", format_session_status(resp.status));
    println!("participants: {}", resp.participants.join(", "));
    println!("round: {}/{}", resp.current_round, resp.total_rounds);
    Ok(())
}

async fn run_join(
    addr: &str,
    session_id: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let resp = client::join(addr, name, session_id).await?;
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
    print_results(&resp);
    Ok(())
}

fn print_results(results: &council_proto::ResultsResponse) {
    let outcome = match results.outcome {
        x if x == council_proto::Outcome::Approved as i32 => "APPROVED",
        x if x == council_proto::Outcome::Rejected as i32 => "REJECTED",
        _ => "UNKNOWN",
    };
    println!("outcome: {}", outcome);
    println!("yay_count: {}", results.yay_count);
    println!("nay_count: {}", results.nay_count);
    for v in &results.votes {
        let choice = match v.choice {
            x if x == council_proto::VoteChoice::Yay as i32 => "yay",
            x if x == council_proto::VoteChoice::Nay as i32 => "nay",
            _ => "unknown",
        };
        println!("vote: {} {} \"{}\"", v.participant, choice, v.reason);
    }
    if !results.decision_record.is_empty() {
        println!("---");
        println!("{}", results.decision_record);
    }
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
