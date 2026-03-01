use std::time::Duration;

use crate::error::DaemonError;

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub rounds: u32,
    pub min_participants: u32,
    pub join_timeout: Duration,
    pub turn_timeout: Duration,
}

impl SessionConfig {
    pub fn validate(&self) -> Result<(), DaemonError> {
        if self.rounds == 0 || self.rounds > 10 {
            return Err(DaemonError::Config(
                "rounds must be between 1 and 10".to_string(),
            ));
        }
        if self.min_participants == 0 {
            return Err(DaemonError::Config(
                "min-participants must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}
