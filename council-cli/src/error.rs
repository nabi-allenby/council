use std::fmt;

#[derive(Debug)]
pub enum CliError {
    Connection(String),
    Rpc(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Connection(msg) => write!(f, "connection error: {}", msg),
            CliError::Rpc(msg) => write!(f, "rpc error: {}", msg),
        }
    }
}

impl std::error::Error for CliError {}

impl From<tonic::transport::Error> for CliError {
    fn from(e: tonic::transport::Error) -> Self {
        CliError::Connection(e.to_string())
    }
}

impl From<tonic::Status> for CliError {
    fn from(e: tonic::Status) -> Self {
        CliError::Rpc(format!("{}: {}", e.code(), e.message()))
    }
}
