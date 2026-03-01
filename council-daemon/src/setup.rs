use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::process::Command;

use crate::daemon_config::DaemonConfig;

const HOOK_MAIN: &str = include_str!("hooks/hook.sh");
const HOOK_BASIC: &str = include_str!("hooks/basic.sh");

/// Create config directory, write defaults, install hooks, and start the daemon.
pub fn run_setup(port: Option<u16>) -> Result<(), Box<dyn std::error::Error>> {
    let config_dir = DaemonConfig::config_dir().ok_or("cannot determine config directory")?;
    let hooks_dir = DaemonConfig::hooks_dir().ok_or("cannot determine hooks directory")?;

    // Create directories
    fs::create_dir_all(&hooks_dir)?;
    eprintln!("Created {}", config_dir.display());

    // Write config (preserve existing if present)
    let config_path = DaemonConfig::config_path().ok_or("cannot determine config path")?;
    let is_new_config = !config_path.exists();
    let mut config = if is_new_config {
        DaemonConfig::default()
    } else {
        eprintln!(
            "Config already exists, preserving {}",
            config_path.display()
        );
        DaemonConfig::load()
    };

    // Apply port override if specified
    if let Some(p) = port {
        config.daemon.port = p;
    }

    // For new configs, prompt for LLM agent command first, then write everything
    // in a single pass. The [agent] section is read by the CLI, not the daemon.
    if is_new_config {
        let agent_command = select_agent_command();
        let mut content = toml::to_string_pretty(&config)?;
        // writeln! on String is infallible; unwrap is safe here.
        writeln!(content, "\n[agent]\ncommand = {:?}", agent_command).unwrap();
        fs::write(&config_path, content)?;
    } else {
        config.save()?;
    }

    eprintln!("Wrote {}", config_path.display());

    // Install hook scripts (skip if already exist to preserve customizations)
    install_hook(&hooks_dir, "hook.sh", HOOK_MAIN)?;
    install_hook(&hooks_dir, "basic.sh", HOOK_BASIC)?;

    // Start daemon in background
    start_daemon(&config)?;

    Ok(())
}

/// Install a hook script, skipping if a user-modified version already exists.
fn install_hook(
    hooks_dir: &std::path::Path,
    name: &str,
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = hooks_dir.join(name);
    if path.exists() {
        eprintln!("Hook already exists, skipping {}", path.display());
    } else {
        fs::write(&path, content)?;
        set_executable(&path)?;
        eprintln!("Installed {}", path.display());
    }
    Ok(())
}

/// Start the daemon as a background process.
fn start_daemon(config: &DaemonConfig) -> Result<(), Box<dyn std::error::Error>> {
    let pid_path = DaemonConfig::pid_path().ok_or("cannot determine PID path")?;
    let log_path = DaemonConfig::log_path().ok_or("cannot determine log path")?;

    // Check if already running
    if is_daemon_running() {
        eprintln!(
            "Daemon is already running (PID file: {})",
            pid_path.display()
        );
        return Ok(());
    }

    let log_file = fs::File::create(&log_path)?;
    let log_stderr = log_file.try_clone()?;

    let exe = std::env::current_exe()?;
    let mut cmd = Command::new(exe);
    cmd.arg("run")
        .arg("--port")
        .arg(config.daemon.port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_stderr);

    // Detach from terminal on Unix
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = cmd.spawn()?;
    let pid = child.id();

    fs::write(&pid_path, pid.to_string())?;
    eprintln!(
        "Daemon started (PID {}, logs at {})",
        pid,
        log_path.display()
    );

    Ok(())
}

/// Check if the daemon is running and print status.
pub fn run_status() -> Result<(), Box<dyn std::error::Error>> {
    let pid_path = DaemonConfig::pid_path().ok_or("cannot determine PID path")?;

    if !pid_path.exists() {
        println!("stopped (no PID file)");
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse()?;

    if process_alive(pid) {
        println!("running (PID {})", pid);
    } else {
        println!("stopped (stale PID file, process {} not found)", pid);
        // Clean up stale PID file
        let _ = fs::remove_file(&pid_path);
    }

    Ok(())
}

/// Gracefully stop the daemon.
pub fn run_stop() -> Result<(), Box<dyn std::error::Error>> {
    let pid_path = DaemonConfig::pid_path().ok_or("cannot determine PID path")?;

    if !pid_path.exists() {
        eprintln!("Daemon is not running (no PID file)");
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid: u32 = pid_str.trim().parse()?;

    if !process_alive(pid) {
        eprintln!("Daemon is not running (stale PID {})", pid);
        fs::remove_file(&pid_path)?;
        return Ok(());
    }

    // Send SIGTERM
    #[cfg(unix)]
    {
        let status = Command::new("kill").arg(pid.to_string()).status()?;
        if status.success() {
            eprintln!("Sent SIGTERM to daemon (PID {})", pid);
        } else {
            return Err(format!("failed to send signal to PID {}", pid).into());
        }
    }

    #[cfg(not(unix))]
    {
        return Err("stop is only supported on Unix systems".into());
    }

    // Wait briefly for process to exit, then clean up PID file
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if !process_alive(pid) {
            let _ = fs::remove_file(&pid_path);
            eprintln!("Daemon stopped");
            return Ok(());
        }
    }

    eprintln!(
        "Warning: daemon (PID {}) did not exit within 2 seconds after SIGTERM",
        pid
    );

    Ok(())
}

/// Tail the daemon log file.
pub fn run_logs(follow: bool, lines: usize) -> Result<(), Box<dyn std::error::Error>> {
    let log_path = DaemonConfig::log_path().ok_or("cannot determine log path")?;

    if !log_path.exists() {
        eprintln!("No log file found at {}", log_path.display());
        return Ok(());
    }

    if follow {
        #[cfg(unix)]
        {
            let status = Command::new("tail")
                .arg("-f")
                .arg("-n")
                .arg(lines.to_string())
                .arg(&log_path)
                .status()?;
            if !status.success() {
                return Err("tail command failed".into());
            }
        }
        #[cfg(not(unix))]
        {
            let _ = lines;
            return Err("follow mode is only supported on Unix systems".into());
        }
    } else {
        // Read last N lines using a ring buffer to avoid loading entire file
        let file = fs::File::open(&log_path)?;
        let reader = BufReader::new(file);
        let mut ring: VecDeque<String> = VecDeque::with_capacity(lines);
        for line in reader.lines() {
            let line = line?;
            if ring.len() == lines {
                ring.pop_front();
            }
            ring.push_back(line);
        }
        for line in &ring {
            println!("{}", line);
        }
    }

    Ok(())
}

/// Check if the daemon process is running by reading the PID file.
fn is_daemon_running() -> bool {
    let pid_path = match DaemonConfig::pid_path() {
        Some(p) => p,
        None => return false,
    };

    let pid_str = match fs::read_to_string(&pid_path) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => return false,
    };

    process_alive(pid)
}

/// Check if a process with the given PID is alive.
fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // kill(pid, 0) checks if process exists without sending a signal
        let ret = unsafe { libc::kill(pid as i32, 0) };
        ret == 0
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

/// Set file as executable on Unix.
#[cfg(unix)]
fn set_executable(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

/// Prompt the user to select their LLM agent command.
///
/// In a TTY, shows a numbered menu on stderr and reads from stdin.
/// In a non-TTY (piped/CI), silently returns the default `"claude -p"`.
fn select_agent_command() -> String {
    let default = "claude -p";

    if !std::io::stdin().is_terminal() {
        return default.to_string();
    }

    let options = [
        ("Claude Code", "claude -p"),
        ("Gemini CLI", "gemini -p"),
        ("OpenAI Codex", "codex exec -"),
        ("Ollama", "ollama run <model>"),
        ("LLM", "llm"),
    ];

    eprintln!("\nSelect your LLM agent command:");
    for (i, (name, cmd)) in options.iter().enumerate() {
        eprintln!("  {}) {:<16} ({})", i + 1, name, cmd);
    }
    eprintln!("  {}) Custom", options.len() + 1);

    let choice = read_line_prompt(&format!("Choice [1-{}]: ", options.len() + 1));
    let choice = choice.trim();

    match choice.parse::<usize>() {
        Ok(n) if n >= 1 && n <= options.len() => {
            let (name, cmd) = options[n - 1];
            if name == "Ollama" {
                let model = read_line_prompt("Ollama model name [llama3.2]: ");
                let model = model.trim();
                let model = if model.is_empty() { "llama3.2" } else { model };
                return format!("ollama run {}", model);
            }
            cmd.to_string()
        }
        Ok(n) if n == options.len() + 1 => {
            let cmd = read_line_prompt("Enter full command: ");
            let cmd = cmd.trim();
            if cmd.is_empty() {
                default.to_string()
            } else {
                cmd.to_string()
            }
        }
        _ => {
            eprintln!("Invalid choice, using default: {}", default);
            default.to_string()
        }
    }
}

/// Print a prompt to stderr and read a line from stdin.
fn read_line_prompt(prompt: &str) -> String {
    eprint!("{}", prompt);
    let _ = std::io::stderr().flush();
    let mut buf = String::new();
    let _ = std::io::stdin().read_line(&mut buf);
    buf
}
