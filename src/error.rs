use std::fmt;

#[derive(Debug)]
pub enum CouncilError {
    FileNotFound(String),
    Validation(String),
    ApiError(String),
    RetryExhausted(String),
    NonBinaryQuestion {
        rationale: String,
        suggestion: Option<String>,
    },
}

impl fmt::Display for CouncilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CouncilError::FileNotFound(msg) => write!(f, "{}", msg),
            CouncilError::Validation(msg) => write!(f, "{}", msg),
            CouncilError::ApiError(msg) => write!(f, "{}", msg),
            CouncilError::RetryExhausted(msg) => write!(f, "{}", msg),
            CouncilError::NonBinaryQuestion {
                rationale,
                ..
            } => {
                write!(f, "Cannot frame as a binary vote: {}", rationale)
            }
        }
    }
}

impl std::error::Error for CouncilError {}
