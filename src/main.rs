use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use clap::Parser;

use council::config::{load_config, Backend};
use council::error::CouncilError;
use council::orchestrator::Orchestrator;
use council::output::format_decision_record;
use council::report::save_report;

#[derive(Parser)]
#[command(name = "council")]
#[command(about = "Run a council discussion on a question using AI agents")]
struct Cli {
    /// The question for the council to discuss
    question: String,

    /// Show turns and votes during execution
    #[arg(short, long)]
    verbose: bool,

    /// Number of discussion rounds (overrides config)
    #[arg(short, long)]
    rounds: Option<u32>,

    /// Comma-separated agent rotation order (overrides config)
    #[arg(long)]
    rotation: Option<String>,

    /// Model name to use for all agents (overrides config)
    #[arg(short, long)]
    model: Option<String>,

    /// Comma-separated agent:tool pairs (overrides config)
    #[arg(long)]
    tools: Option<String>,

    /// Agent backend: 'agent-sdk' (Claude CLI) or 'api' (direct Anthropic API)
    #[arg(short, long, value_parser = ["agent-sdk", "api"])]
    backend: Option<String>,

    /// Skip motion crafting and use the original question directly
    #[arg(long)]
    skip_motion: bool,
}

fn main() {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    let agents_dir = PathBuf::from("agents");
    let prompts_dir = PathBuf::from("prompts");
    let logs_dir = PathBuf::from("logs");

    // Load config
    let mut config = match load_config(&agents_dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {}", e);
            process::exit(1);
        }
    };

    // Apply CLI overrides
    if let Some(rounds) = cli.rounds {
        config.rounds = rounds;
    }
    if let Some(model) = &cli.model {
        config.model = model.clone();
    }
    if let Some(rotation) = &cli.rotation {
        config.rotation = rotation.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let Some(tools) = &cli.tools {
        config.tools.clear();
        for pair in tools.split(',') {
            let parts: Vec<&str> = pair.trim().splitn(2, ':').collect();
            if parts.len() == 2 {
                config
                    .tools
                    .entry(parts[0].trim().to_string())
                    .or_default()
                    .push(parts[1].trim().to_string());
            }
        }
    }
    if let Some(backend) = &cli.backend {
        config.backend = match backend.as_str() {
            "agent-sdk" => Backend::AgentSdk,
            "api" => Backend::Api,
            _ => {
                eprintln!("Error: invalid backend '{}'", backend);
                process::exit(1);
            }
        };
    }

    // API backend requires ANTHROPIC_API_KEY; SDK backend uses Claude CLI
    if config.backend == Backend::Api && std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!(
            "Error: ANTHROPIC_API_KEY is not set.\n\
             Add it to a .env file:\n\
               echo 'ANTHROPIC_API_KEY=your-key-here' > .env\n\
             Or export it directly:\n\
               export ANTHROPIC_API_KEY='your-key-here'"
        );
        process::exit(1);
    }

    // Create orchestrator
    let orchestrator =
        match Orchestrator::new(config, &agents_dir, &prompts_dir, cli.verbose, cli.skip_motion) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("Error: {}", e);
                process::exit(1);
            }
        };

    // Run session
    let session = match orchestrator.run(&cli.question) {
        Ok(s) => s,
        Err(CouncilError::NonBinaryQuestion {
            rationale,
            suggestion: Some(suggested),
        }) => {
            eprintln!("Your question is hard to frame as a binary vote.");
            eprintln!("Reason: {}", rationale);
            eprintln!("\nSuggested motion: {}", suggested);
            eprint!("Use this motion? [y/N] ");
            io::stderr().flush().ok();

            let mut input = String::new();
            io::stdin().read_line(&mut input).unwrap_or(0);

            if input.trim().eq_ignore_ascii_case("y") {
                match orchestrator.run_with_motion(&cli.question, suggested) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error during council discussion: {}", e);
                        process::exit(1);
                    }
                }
            } else {
                eprintln!("Aborted. Rephrase your question or use --skip-motion.");
                process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error during council discussion: {}", e);
            process::exit(1);
        }
    };

    // Print decision record
    println!("{}", format_decision_record(&session));

    // Save report
    match save_report(&session, &logs_dir) {
        Ok(path) => println!("\nFull report saved to: {}", path.display()),
        Err(e) => eprintln!("Warning: Could not save report: {}", e),
    }
}
