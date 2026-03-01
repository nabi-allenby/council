use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, RwLock};
use tonic::{Request, Response, Status};

use council_proto::council_server::Council;
use council_proto::{
    CreateSessionRequest, CreateSessionResponse, GetSessionRequest, GetSessionResponse,
    JoinRequest, JoinResponse, ListSessionsRequest, ListSessionsResponse, RespondRequest,
    RespondResponse, ResultsRequest, ResultsResponse, SessionSummary, VoteRecord, VoteRequest,
    VoteResponse, WaitRequest, WaitResponse, WaitStatus,
};

use crate::config::SessionConfig;
use crate::types::{Participant, Session, SessionStatus, Turn, Vote, VoteChoice};

struct SessionState {
    session: RwLock<Session>,
    version_tx: watch::Sender<u64>,
    config: SessionConfig,
}

/// Holds up to `MAX_SESSIONS` sessions. When full, the oldest session is
/// evicted (rolling window) to bound memory usage on long-running daemons.
struct SessionStore {
    sessions: RwLock<HashMap<String, Arc<SessionState>>>,
    order: RwLock<VecDeque<String>>,
}

const MAX_SESSIONS: usize = 10;

impl SessionStore {
    fn new() -> Self {
        SessionStore {
            sessions: RwLock::new(HashMap::new()),
            order: RwLock::new(VecDeque::new()),
        }
    }

    async fn get(&self, session_id: &str) -> Result<Arc<SessionState>, Status> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| Status::not_found("unknown session"))
    }

    async fn insert(&self, session_id: String, state: Arc<SessionState>) {
        let mut sessions = self.sessions.write().await;
        let mut order = self.order.write().await;

        // Evict to stay within the limit. Prefer completed sessions first,
        // then fall back to oldest (FIFO).
        while order.len() >= MAX_SESSIONS {
            // Try to find a completed session to evict
            let evict_idx = {
                let mut completed_idx = None;
                for (i, id) in order.iter().enumerate() {
                    if let Some(s) = sessions.get(id) {
                        // try_read avoids deadlock — skip if locked
                        if let Ok(sess) = s.session.try_read() {
                            if sess.status == SessionStatus::Completed {
                                completed_idx = Some(i);
                                break;
                            }
                        }
                    }
                }
                // Fall back to oldest if no completed session found
                completed_idx.unwrap_or(0)
            };

            if let Some(old_id) = order.remove(evict_idx) {
                sessions.remove(&old_id);
                eprintln!("Evicting session {}", old_id);
            }
        }

        order.push_back(session_id.clone());
        sessions.insert(session_id, state);
    }

    async fn remove(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        let mut order = self.order.write().await;
        sessions.remove(session_id);
        order.retain(|id| id != session_id);
    }

    async fn list_summaries(&self) -> Vec<SessionSummary> {
        let sessions = self.sessions.read().await;
        let mut summaries = Vec::with_capacity(sessions.len());
        for (_, state) in sessions.iter() {
            let session = state.session.read().await;
            summaries.push(SessionSummary {
                session_id: session.id.clone(),
                question: session.question.clone(),
                status: proto_session_status(&session.status),
                participant_count: session.participants.len() as u32,
            });
        }
        summaries
    }
}

pub struct CouncilService {
    store: Arc<SessionStore>,
}

impl Default for CouncilService {
    fn default() -> Self {
        Self::new()
    }
}

impl CouncilService {
    pub fn new() -> Self {
        CouncilService {
            store: Arc::new(SessionStore::new()),
        }
    }
}

#[allow(clippy::result_large_err)]
fn validate_token(session: &Session, name: &str, token: &str) -> Result<(), Status> {
    match session.participants.iter().find(|p| p.name == name) {
        Some(p) if p.token == token => Ok(()),
        Some(_) => Err(Status::permission_denied("invalid participant token")),
        None => Err(Status::not_found("participant not found")),
    }
}

fn spawn_lobby_timeout(state: Arc<SessionState>, store: Arc<SessionStore>) {
    let timeout = state.config.join_timeout;
    let turn_timeout = state.config.turn_timeout;

    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        let mut session = state.session.write().await;
        if session.status != SessionStatus::LobbyOpen {
            // Session already started (min_participants reached before timeout).
        } else if session.participants.is_empty() {
            let id = session.id.clone();
            drop(session);
            eprintln!("Lobby timeout: no participants joined. Removing session {}.", id);
            store.remove(&id).await;
        } else {
            eprintln!(
                "Lobby timeout reached. Starting with {} participant(s).",
                session.participants.len()
            );
            session.start_discussion();
            let round = session.current_round;
            let idx = session.current_speaker_idx;
            state.version_tx.send_modify(|v| *v += 1);
            drop(session);
            spawn_turn_timeout(state, turn_timeout, round, idx);
        }
    });
}

fn spawn_turn_timeout(
    state: Arc<SessionState>,
    timeout: Duration,
    expected_round: u32,
    expected_idx: usize,
) {
    let state2 = state.clone();
    tokio::spawn(async move {
        tokio::time::sleep(timeout).await;
        let mut session = state2.session.write().await;
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
            state2.version_tx.send_modify(|v| *v += 1);
            drop(session);
            if still_in_progress {
                spawn_turn_timeout(state, timeout, round, idx);
            }
        }
    });
}

async fn wait_for_actionable(
    state: &Arc<SessionState>,
    req: &WaitRequest,
) -> Result<Response<WaitResponse>, Status> {
    // Validate once upfront — session_id and token don't change mid-session.
    {
        let session = state.session.read().await;
        validate_token(&session, &req.name, &req.participant_token)?;
    }

    let mut rx = state.version_tx.subscribe();
    loop {
        {
            let session = state.session.read().await;
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

#[tonic::async_trait]
impl Council for CouncilService {
    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let req = request.into_inner();

        if req.question.trim().is_empty() {
            return Err(Status::invalid_argument("question is required"));
        }
        if req.question.chars().count() > 1000 {
            return Err(Status::invalid_argument(
                "question must be at most 1000 characters",
            ));
        }

        let rounds = if req.rounds == 0 { 2 } else { req.rounds };
        let min_participants = if req.min_participants == 0 {
            3
        } else {
            req.min_participants
        };
        let join_timeout_secs = if req.join_timeout_seconds == 0 {
            60
        } else {
            req.join_timeout_seconds
        };
        let turn_timeout_secs = if req.turn_timeout_seconds == 0 {
            120
        } else {
            req.turn_timeout_seconds
        };

        let config = SessionConfig {
            rounds,
            min_participants,
            join_timeout: Duration::from_secs(join_timeout_secs as u64),
            turn_timeout: Duration::from_secs(turn_timeout_secs as u64),
        };
        config
            .validate()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(session_id.clone(), req.question.clone(), rounds);
        let (version_tx, _) = watch::channel(0u64);

        let state = Arc::new(SessionState {
            session: RwLock::new(session),
            version_tx,
            config,
        });

        self.store.insert(session_id.clone(), state.clone()).await;
        spawn_lobby_timeout(state, self.store.clone());

        eprintln!("Session created: {} — \"{}\"", session_id, req.question);

        Ok(Response::new(CreateSessionResponse { session_id }))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let req = request.into_inner();
        let state = self.store.get(&req.session_id).await?;
        let session = state.session.read().await;

        Ok(Response::new(GetSessionResponse {
            session_id: session.id.clone(),
            question: session.question.clone(),
            status: proto_session_status(&session.status),
            participants: session.participant_names(),
            current_round: session.current_round,
            total_rounds: session.total_rounds,
        }))
    }

    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let sessions = self.store.list_summaries().await;
        Ok(Response::new(ListSessionsResponse { sessions }))
    }

    async fn join(&self, request: Request<JoinRequest>) -> Result<Response<JoinResponse>, Status> {
        let req = request.into_inner();

        if req.name.trim().is_empty() {
            return Err(Status::invalid_argument("name is required"));
        }
        if req.name.chars().count() > 100 {
            return Err(Status::invalid_argument(
                "name must be at most 100 characters",
            ));
        }
        if req.session_id.trim().is_empty() {
            return Err(Status::invalid_argument("session_id is required"));
        }

        let state = self.store.get(&req.session_id).await?;
        let mut session = state.session.write().await;

        if session.status != SessionStatus::LobbyOpen {
            return Err(Status::failed_precondition("lobby is closed"));
        }

        if session.participants.iter().any(|p| p.name == req.name) {
            return Err(Status::already_exists("participant name already taken"));
        }

        // Random UUID token — no TTL, signing, or IP binding. Sufficient for
        // single-machine trusted hooks; revisit if exposed to untrusted networks.
        let token = uuid::Uuid::new_v4().to_string();
        session.participants.push(Participant {
            name: req.name.clone(),
            token: token.clone(),
        });

        eprintln!("Participant joined: {}", req.name);

        // Check if we should start before building the response,
        // so the joining participant sees the correct post-transition status.
        let auto_start = session.participants.len() as u32 >= state.config.min_participants;
        if auto_start {
            eprintln!(
                "Min participants reached ({}). Starting discussion.",
                session.participants.len()
            );
            session.start_discussion();
        }

        let response = JoinResponse {
            session_id: session.id.clone(),
            question: session.question.clone(),
            participants: session.participant_names(),
            status: proto_session_status(&session.status),
            rounds: session.total_rounds,
            min_participants: state.config.min_participants,
            participant_token: token,
        };

        let round = session.current_round;
        let idx = session.current_speaker_idx;
        let turn_timeout = state.config.turn_timeout;
        drop(session);
        state.version_tx.send_modify(|v| *v += 1);
        if auto_start {
            spawn_turn_timeout(state, turn_timeout, round, idx);
        }

        Ok(Response::new(response))
    }

    async fn wait(&self, request: Request<WaitRequest>) -> Result<Response<WaitResponse>, Status> {
        let req = request.into_inner();
        let state = self.store.get(&req.session_id).await?;

        let timeout_secs = if req.timeout_seconds > 0 {
            req.timeout_seconds
        } else {
            30
        };
        let timeout = Duration::from_secs(timeout_secs as u64);

        let result = tokio::time::timeout(timeout, wait_for_actionable(&state, &req)).await;

        match result {
            Ok(response) => response,
            Err(_) => {
                // Timeout - return current status
                let session = state.session.read().await;
                validate_token(&session, &req.name, &req.participant_token)?;
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
        if req.position.chars().count() > 300 {
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

        let state = self.store.get(&req.session_id).await?;
        let mut session = state.session.write().await;
        validate_token(&session, &req.name, &req.participant_token)?;

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
        let turn_timeout = state.config.turn_timeout;
        drop(session);

        state.version_tx.send_modify(|v| *v += 1);
        if in_progress {
            spawn_turn_timeout(state, turn_timeout, round, idx);
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
        if req.reason.chars().count() > 500 {
            return Err(Status::invalid_argument(
                "reason must be at most 500 characters",
            ));
        }

        let choice = match req.choice {
            x if x == council_proto::VoteChoice::Yay as i32 => VoteChoice::Yay,
            x if x == council_proto::VoteChoice::Nay as i32 => VoteChoice::Nay,
            _ => return Err(Status::invalid_argument("choice must be YAY or NAY")),
        };

        let state = self.store.get(&req.session_id).await?;
        let mut session = state.session.write().await;
        validate_token(&session, &req.name, &req.participant_token)?;

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

        let completed = if session.all_voted() {
            session.status = SessionStatus::Completed;
            let outcome = session.outcome();
            let yays = session
                .votes
                .iter()
                .filter(|v| v.choice == VoteChoice::Yay)
                .count();
            let nays = session.votes.len() - yays;
            eprintln!("Session complete: {} ({}-{})", outcome.upper(), yays, nays);
            Some(session.clone())
        } else {
            None
        };

        drop(session);
        state.version_tx.send_modify(|v| *v += 1);

        // Best-effort report save (outside write lock to avoid blocking)
        if let Some(ref completed_session) = completed {
            if let Err(e) = crate::report::save_report(completed_session, Path::new("logs")) {
                eprintln!("Warning: failed to save report: {}", e);
            }
        }

        Ok(Response::new(VoteResponse {
            accepted: true,
            message: "vote recorded".to_string(),
        }))
    }

    async fn results(
        &self,
        request: Request<ResultsRequest>,
    ) -> Result<Response<ResultsResponse>, Status> {
        let req = request.into_inner();
        let state = self.store.get(&req.session_id).await?;
        let session = state.session.read().await;

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
