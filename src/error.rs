use std::fmt;

#[derive(Debug)]
pub enum CouncilError {
    FileNotFound(String),
    Validation(String),
    ApiError(String),
    RetryExhausted(String),
    NonBinaryQuestion(String),
}

impl fmt::Display for CouncilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CouncilError::FileNotFound(msg) => write!(f, "{}", msg),
            CouncilError::Validation(msg) => write!(f, "{}", msg),
            CouncilError::ApiError(msg) => write!(f, "{}", msg),
            CouncilError::RetryExhausted(msg) => write!(f, "{}", msg),
            CouncilError::NonBinaryQuestion(msg) => write!(
                f,
                "Your question cannot be framed as a binary (yay/nay) vote.\n\
                 Reason: {}\n\
                 Please rephrase as a yes/no proposal, or use --skip-motion to bypass motion crafting.",
                msg
            ),
        }
    }
}

impl std::error::Error for CouncilError {}
