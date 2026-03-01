use std::fmt;

#[derive(Debug)]
pub enum DaemonError {
    Io(String),
    Config(String),
}

impl fmt::Display for DaemonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DaemonError::Io(msg) => write!(f, "{}", msg),
            DaemonError::Config(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for DaemonError {}
