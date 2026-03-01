use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, RwLock};
use tonic::{Request, Response, Status};

use council_proto::council_server::Council;
use council_proto::{
    JoinRequest, JoinResponse, RespondRequest, RespondResponse, ResultsRequest, ResultsResponse,
    VoteRecord, VoteRequest, VoteResponse, WaitRequest, WaitResponse, WaitStatus,
};

use crate::config::DaemonConfig;
use crate::types::{Participant, Session, SessionStatus, Turn, Vote, VoteChoice};

struct SharedState {
    session: RwLock<Session>,
    version_tx: watch::Sender<u64>,
}

pub struct CouncilService {
    shared: Arc<SharedState>,
    config: DaemonConfig,
}

impl CouncilService {
    pub fn new(question: String, config: DaemonConfig) -> Self {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(session_id, question, config.rounds);
        let (version_tx, _) = watch::channel(0u64);

        CouncilService {
            shared: Arc::new(SharedState {
                session: RwLock::new(session),
                version_tx,
            }),
            config,
        }
    }

    fn notify_state_change(&self) {
        self.shared.version_tx.send_modify(|v| *v += 1);
    }

    #[allow(clippy::result_large_err)]
    fn validate_token(session: &Session, name: &str, token: &str) -> Result<(), Status> {
        match session.participants.iter().find(|p| p.name == name) {
            Some(p) if p.token == token => Ok(()),
            Some(_) => Err(Status::permission_denied("invalid participant token")),
            None => Err(Status::not_found("participant not found")),
        }
    }

    pub fn spawn_lobby_timeout(&self) {
        let shared = self.shared.clone();
        let timeout = self.config.join_timeout;
        let turn_timeout = self.config.turn_timeout;

        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            let mut session = shared.session.write().await;
            if session.status == SessionStatus::LobbyOpen && !session.participants.is_empty() {
                eprintln!(
                    "Lobby timeout reached. Starting with {} participant(s).",
                    session.participants.len()
                );
                session.start_discussion();
                let round = session.current_round;
                let idx = session.current_speaker_idx;
                shared.version_tx.send_modify(|v| *v += 1);
                drop(session);
                spawn_turn_timeout(shared, turn_timeout, round, idx);
            }
        });
    }

    async fn wait_for_actionable(
        &self,
        req: &WaitRequest,
    ) -> Result<Response<WaitResponse>, Status> {
        let mut rx = self.shared.version_tx.subscribe();
        loop {
            {
                let session = self.shared.session.read().await;
                Self::validate_token(&session, &req.name, &req.participant_token)?;

                let response = build_wait_response(&session, &req.name);
                let status = response.status;
                if status == WaitStatus::YourTurn as i32
                    || status == WaitStatus::VotePhase as i32
                    || status == WaitStatus::Complete as i32
                {
                    return Ok(Response::new(response));
                }
            }
            if rx.changed().await.is_err() {
                return Err(Status::internal("session closed"));
            }
        }
    }
}

fn spawn_turn_timeout(
    shared: Arc<SharedState>,
    timeout: Duration,
    expected_round: u32,
    expected_idx: usize,
) {
    let shared2 = shared.clone();
    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        let mut session = shared2.session.write().await;
        if session.status == SessionStatus::InProgress
            && session.current_round == expected_round
            && session.current_speaker_idx == expected_idx
        {
            let speaker = session.current_speaker().unwrap_or("unknown").to_string();
            eprintln!("Turn timeout: skipping {}", speaker);
            session.advance_speaker();
            let round = session.current_round;
            let idx = session.current_speaker_idx;
            let still_in_progress = session.status == SessionStatus::InProgress;
            shared2.version_tx.send_modify(|v| *v += 1);
            drop(session);
            if still_in_progress {
                spawn_turn_timeout(shared, timeout, round, idx);
            }
        }
    });
}

#[tonic::async_trait]
impl Council for CouncilService {
    async fn join(&self, request: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();

        if req.name.trim().is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }

        let mut session = self.shared.session.write().await;

        if session.status != SessionStatus::LobbyOpen {
            return Err(Status::failed_precondition("lobby is closed"));
        }

        if session.participants.iter().any(|p| p.name == req.name) {
            return Err(Status::already_exists("participant name already taken"));
        }

        let token = uuid::Uuid::new_v4().to_string();
        session.participants.push(Participant {
            name: req.name.clone(),
            token: token.clone(),
        });

        eprintln!("Participant joined: {}", req.name);

        let response = JoinResponse {
            session_id: session.id.clone(),
            question: session.question.clone(),
            participants: session.participant_names(),
            status: proto_session_status(&session.status),
            rounds: session.total_rounds,
            min_participants: self.config.min_participants,
            participant_token: token,
        };

        // Check if we should start
        if session.participants.len() as u32 >= self.config.min_participants {
            eprintln!(
                "Min participants reached ({}). Starting discussion.",
                session.participants.len()
            );
            session.start_discussion();
            let round = session.current_round;
            let idx = session.current_speaker_idx;
            drop(session);
            self.notify_state_change();
            spawn_turn_timeout(self.shared.clone(), self.config.turn_timeout, round, idx);
        } else {
            drop(session);
            self.notify_state_change();
        }

        Ok(Response::new(response))
    }

    async fn wait(&self, request: Request<WaitRequest>) -> Result<Response<WaitResponse>, Status> {
        let req = request.into_inner();
        let timeout_secs = if req.timeout_seconds > 0 {
            req.timeout_seconds
        } else {
            30
        };
        let timeout = Duration::from_secs(timeout_secs as u64);

        let result = tokio::time::timeout(timeout, self.wait_for_actionable(&req)).await;

        match result {
            Ok(response) => response,
            Err(_) => {
                // Timeout - return current status
                let session = self.shared.session.read().await;
                Self::validate_token(&session, &req.name, &req.participant_token)?;
                Ok(Response::new(build_wait_response(&session, &req.name)))
            }
        }
    }

    async fn respond(
        &self,
        request: Request<RespondRequest>,
    ) -> Result<Response<RespondResponse>, Status> {
        let req = request.into_inner();

        if req.position.trim().is_empty() {
            return Err(Status::invalid_argument("position is required"));
        }
        if req.position.len() > 300 {
            return Err(Status::invalid_argument(
                "position must be at most 300 characters",
            ));
        }
        if req.reasoning.is_empty() || req.reasoning.len() > 5 {
            return Err(Status::invalid_argument("reasoning must have 1-5 items"));
        }
        if req.concerns.len() > 5 {
            return Err(Status::invalid_argument("concerns must have 0-5 items"));
        }

        let mut session = self.shared.session.write().await;
        Self::validate_token(&session, &req.name, &req.participant_token)?;

        if session.status != SessionStatus::InProgress {
            return Err(Status::failed_precondition("not in discussion phase"));
        }

        let current = session.current_speaker().map(|s| s.to_string());
        if current.as_deref() != Some(req.name.as_str()) {
            return Err(Status::failed_precondition(format!(
                "not your turn (current speaker: {})",
                current.unwrap_or_else(|| "none".to_string())
            )));
        }

        eprintln!(
            "Response from {} (Round {})",
            req.name, session.current_round
        );

        let turn = Turn {
            participant: req.name.clone(),
            round: session.current_round,
            position: req.position,
            reasoning: req.reasoning,
            concerns: req.concerns,
        };
        session.turns.push(turn);
        session.advance_speaker();

        let next_step = match session.status {
            SessionStatus::Voting => "wait for vote phase".to_string(),
            SessionStatus::InProgress => {
                let speaker = session.current_speaker().unwrap_or("unknown");
                format!("wait for next turn (current: {})", speaker)
            }
            _ => "session complete".to_string(),
        };

        let round = session.current_round;
        let idx = session.current_speaker_idx;
        let in_progress = session.status == SessionStatus::InProgress;
        drop(session);

        self.notify_state_change();
        if in_progress {
            spawn_turn_timeout(self.shared.clone(), self.config.turn_timeout, round, idx);
        }

        Ok(Response::new(RespondResponse {
            accepted: true,
            next_step,
        }))
    }

    async fn vote(&self, request: Request<VoteRequest>) -> Result<Response<VoteResponse>, Status> {
        let req = request.into_inner();

        if req.reason.trim().is_empty() {
            return Err(Status::invalid_argument("reason is required"));
        }
        if req.reason.len() > 500 {
            return Err(Status::invalid_argument(
                "reason must be at most 500 characters",
            ));
        }

        let choice = match req.choice {
            x if x == council_proto::VoteChoice::Yay as i32 => VoteChoice::Yay,
            x if x == council_proto::VoteChoice::Nay as i32 => VoteChoice::Nay,
            _ => return Err(Status::invalid_argument("choice must be YAY or NAY")),
        };

        let mut session = self.shared.session.write().await;
        Self::validate_token(&session, &req.name, &req.participant_token)?;

        if session.status != SessionStatus::Voting {
            return Err(Status::failed_precondition("not in voting phase"));
        }

        if session.has_voted(&req.name) {
            return Err(Status::already_exists("already voted"));
        }

        eprintln!("Vote from {}: {}", req.name, choice);

        session.votes.push(Vote {
            participant: req.name.clone(),
            choice,
            reason: req.reason,
        });

        if session.all_voted() {
            session.status = SessionStatus::Completed;
            let outcome = session.outcome();
            let yays = session
                .votes
                .iter()
                .filter(|v| v.choice == VoteChoice::Yay)
                .count();
            let nays = session.votes.len() - yays;
            eprintln!("Session complete: {} ({}-{})", outcome.upper(), yays, nays);

            // Best-effort report save
            if let Err(e) = crate::report::save_report(&session, Path::new("logs")) {
                eprintln!("Warning: failed to save report: {}", e);
            }
        }

        drop(session);
        self.notify_state_change();

        Ok(Response::new(VoteResponse {
            accepted: true,
            message: "vote recorded".to_string(),
        }))
    }

    async fn results(
        &self,
        _request: Request<ResultsRequest>,
    ) -> Result<Response<ResultsResponse>, Status> {
        let session = self.shared.session.read().await;

        if session.status != SessionStatus::Completed {
            return Err(Status::failed_precondition("session not completed"));
        }

        let outcome = session.outcome();
        let yays = session
            .votes
            .iter()
            .filter(|v| v.choice == VoteChoice::Yay)
            .count() as u32;
        let nays = session
            .votes
            .iter()
            .filter(|v| v.choice == VoteChoice::Nay)
            .count() as u32;

        let proto_outcome = match outcome {
            crate::types::Outcome::Approved => council_proto::Outcome::Approved as i32,
            crate::types::Outcome::Rejected => council_proto::Outcome::Rejected as i32,
        };

        let vote_records: Vec<VoteRecord> = session
            .votes
            .iter()
            .map(|v| VoteRecord {
                participant: v.participant.clone(),
                choice: match v.choice {
                    VoteChoice::Yay => council_proto::VoteChoice::Yay as i32,
                    VoteChoice::Nay => council_proto::VoteChoice::Nay as i32,
                },
                reason: v.reason.clone(),
            })
            .collect();

        let decision_record = crate::output::format_decision_record(&session);
        let full_report = crate::report::generate_report(&session);

        Ok(Response::new(ResultsResponse {
            question: session.question.clone(),
            outcome: proto_outcome,
            yay_count: yays,
            nay_count: nays,
            votes: vote_records,
            decision_record,
            full_report,
        }))
    }
}

fn build_wait_response(session: &Session, name: &str) -> WaitResponse {
    let status = match &session.status {
        SessionStatus::LobbyOpen => WaitStatus::Lobby as i32,
        SessionStatus::InProgress => {
            if session.current_speaker() == Some(name) {
                WaitStatus::YourTurn as i32
            } else {
                WaitStatus::Waiting as i32
            }
        }
        SessionStatus::Voting => {
            if session.has_voted(name) {
                WaitStatus::Waiting as i32
            } else {
                WaitStatus::VotePhase as i32
            }
        }
        SessionStatus::Completed => WaitStatus::Complete as i32,
    };

    WaitResponse {
        status,
        current_round: session.current_round,
        total_rounds: session.total_rounds,
        question: session.question.clone(),
        transcript: session.build_transcript(),
        current_speaker: session.current_speaker().unwrap_or("").to_string(),
        participants: session.participant_names(),
    }
}

fn proto_session_status(status: &SessionStatus) -> i32 {
    match status {
        SessionStatus::LobbyOpen => council_proto::SessionStatus::LobbyOpen as i32,
        SessionStatus::InProgress => council_proto::SessionStatus::InProgress as i32,
        SessionStatus::Voting => council_proto::SessionStatus::Voting as i32,
        SessionStatus::Completed => council_proto::SessionStatus::Completed as i32,
    }
}
