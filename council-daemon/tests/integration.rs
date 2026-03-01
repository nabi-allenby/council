use std::time::Duration;

use council_daemon::server::CouncilService;
use council_proto::council_client::CouncilClient;
use council_proto::council_server::CouncilServer;
use council_proto::*;
use tokio::net::TcpListener;
use tonic::transport::Channel;
use tonic::transport::Server;

/// Helper: start a bare daemon on a random port and return the address + a client.
async fn start_daemon() -> (String, CouncilClient<Channel>) {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let service = CouncilService::new();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(CouncilServer::new(service))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = CouncilClient::connect(url.clone()).await.unwrap();
    (url, client)
}

/// Helper: create a session and return session_id
async fn create_session(
    client: &mut CouncilClient<Channel>,
    question: &str,
    rounds: u32,
    min_participants: u32,
    join_timeout_secs: u32,
) -> String {
    let resp = client
        .create_session(CreateSessionRequest {
            question: question.to_string(),
            rounds,
            min_participants,
            join_timeout_seconds: join_timeout_secs,
            turn_timeout_seconds: 120,
        })
        .await
        .unwrap()
        .into_inner();
    resp.session_id
}

/// Helper: join a participant and return (session_id, token)
async fn join(
    client: &mut CouncilClient<Channel>,
    name: &str,
    session_id: &str,
) -> (String, String) {
    let resp = client
        .join(JoinRequest {
            name: name.to_string(),
            session_id: session_id.to_string(),
        })
        .await
        .unwrap()
        .into_inner();
    (resp.session_id, resp.participant_token)
}

/// Helper: wait for a specific status
async fn wait_for_status(
    client: &mut CouncilClient<Channel>,
    session_id: &str,
    name: &str,
    token: &str,
    timeout: u32,
) -> WaitResponse {
    client
        .wait(WaitRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            timeout_seconds: timeout,
            participant_token: token.to_string(),
        })
        .await
        .unwrap()
        .into_inner()
}

/// Helper: submit a response
async fn respond(
    client: &mut CouncilClient<Channel>,
    session_id: &str,
    name: &str,
    token: &str,
    position: &str,
) -> RespondResponse {
    client
        .respond(RespondRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            position: position.to_string(),
            reasoning: vec!["Test reasoning".to_string()],
            concerns: vec![],
            participant_token: token.to_string(),
        })
        .await
        .unwrap()
        .into_inner()
}

/// Helper: cast a vote
async fn vote(
    client: &mut CouncilClient<Channel>,
    session_id: &str,
    name: &str,
    token: &str,
    choice: VoteChoice,
    reason: &str,
) -> VoteResponse {
    client
        .vote(VoteRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            choice: choice as i32,
            reason: reason.to_string(),
            participant_token: token.to_string(),
        })
        .await
        .unwrap()
        .into_inner()
}

// ── Full session test ──

#[tokio::test]
async fn test_full_session_3_participants() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Should we adopt Rust?", 1, 3, 30).await;

    // Join
    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;
    let (_, t3) = join(&mut c3, "Carol", &sid).await;

    // Round 1: Alice's turn
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "Alice", &t1, "Yes, adopt Rust").await;

    // Bob's turn
    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(
        &mut c2,
        &sid,
        "Bob",
        &t2,
        "Adopt Rust for new services only",
    )
    .await;

    // Carol's turn
    let w = wait_for_status(&mut c3, &sid, "Carol", &t3, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c3, &sid, "Carol", &t3, "No, stick with Go").await;

    // Vote phase
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);

    vote(
        &mut c1,
        &sid,
        "Alice",
        &t1,
        VoteChoice::Yay,
        "Rust is safer",
    )
    .await;
    vote(
        &mut c2,
        &sid,
        "Bob",
        &t2,
        VoteChoice::Yay,
        "Worth the investment",
    )
    .await;
    vote(
        &mut c3,
        &sid,
        "Carol",
        &t3,
        VoteChoice::Nay,
        "Too steep a learning curve",
    )
    .await;

    // Results
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::Complete as i32);

    let results = c1
        .results(ResultsRequest {
            session_id: sid.clone(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(results.outcome, Outcome::Approved as i32);
    assert_eq!(results.yay_count, 2);
    assert_eq!(results.nay_count, 1);
    assert_eq!(results.votes.len(), 3);
    assert!(!results.decision_record.is_empty());
    assert!(!results.full_report.is_empty());
}

// ── Join timeout test ──

#[tokio::test]
async fn test_join_timeout_starts_with_fewer_participants() {
    // min_participants=5 but join_timeout=1s, only 2 join
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test question?", 1, 5, 1).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // Wait for lobby timeout to fire (1 second)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Should now be in progress
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert!(
        w.status == WaitStatus::YourTurn as i32 || w.status == WaitStatus::Waiting as i32,
        "Expected discussion to start after timeout, got status: {}",
        w.status
    );

    // Complete the session
    if w.status == WaitStatus::YourTurn as i32 {
        respond(&mut c1, &sid, "Alice", &t1, "Position A").await;
    }
    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "Bob", &t2, "Position B").await;

    // Vote
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);
    vote(&mut c1, &sid, "Alice", &t1, VoteChoice::Yay, "Agree").await;
    vote(&mut c2, &sid, "Bob", &t2, VoteChoice::Nay, "Disagree").await;

    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::Complete as i32);
}

// ── Respond out of turn rejected ──

#[tokio::test]
async fn test_respond_out_of_turn_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test question?", 1, 2, 30).await;

    let (_, _t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // It's Alice's turn, Bob tries to respond
    let result = c2
        .respond(RespondRequest {
            session_id: sid.clone(),
            name: "Bob".to_string(),
            position: "Should not work".to_string(),
            reasoning: vec!["test".to_string()],
            concerns: vec![],
            participant_token: t2.clone(),
        })
        .await;

    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    assert!(status.message().contains("not your turn"));
}

// ── Vote before discussion ends rejected ──

#[tokio::test]
async fn test_vote_before_discussion_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test question?", 1, 2, 30).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Try to vote while still in discussion phase
    let result = c1
        .vote(VoteRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            choice: VoteChoice::Yay as i32,
            reason: "premature vote".to_string(),
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    let status = result.unwrap_err();
    assert_eq!(status.code(), tonic::Code::FailedPrecondition);
    assert!(status.message().contains("not in voting phase"));
}

// ── Results reflect correct majority ──

#[tokio::test]
async fn test_results_reflect_majority() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Should we merge?", 1, 3, 30).await;

    let (_, t1) = join(&mut c1, "A", &sid).await;
    let (_, t2) = join(&mut c2, "B", &sid).await;
    let (_, t3) = join(&mut c3, "C", &sid).await;

    // Discussion round: A, B, C each respond in order
    let w = wait_for_status(&mut c1, &sid, "A", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "A", &t1, "A's position").await;

    let w = wait_for_status(&mut c2, &sid, "B", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "B", &t2, "B's position").await;

    let w = wait_for_status(&mut c3, &sid, "C", &t3, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c3, &sid, "C", &t3, "C's position").await;

    // Vote: 1 yay, 2 nay -> REJECTED
    let w = wait_for_status(&mut c1, &sid, "A", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);

    vote(&mut c1, &sid, "A", &t1, VoteChoice::Yay, "Yes").await;
    vote(&mut c2, &sid, "B", &t2, VoteChoice::Nay, "No").await;
    vote(&mut c3, &sid, "C", &t3, VoteChoice::Nay, "No").await;

    let w = wait_for_status(&mut c1, &sid, "A", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::Complete as i32);

    let results = c1
        .results(ResultsRequest {
            session_id: sid.clone(),
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(results.outcome, Outcome::Rejected as i32);
    assert_eq!(results.yay_count, 1);
    assert_eq!(results.nay_count, 2);
}

// ── Concurrent clients don't deadlock ──

#[tokio::test]
async fn test_concurrent_clients_no_deadlock() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Concurrent test?", 1, 3, 30).await;

    let (_, t1) = join(&mut c1, "P1", &sid).await;
    let (_, t2) = join(&mut c2, "P2", &sid).await;
    let (_, t3) = join(&mut c3, "P3", &sid).await;

    // All three wait concurrently
    let sid_c = sid.clone();
    let t1_c = t1.clone();
    let mut c1_clone = c1.clone();
    let h1 =
        tokio::spawn(async move { wait_for_status(&mut c1_clone, &sid_c, "P1", &t1_c, 10).await });

    let sid_c = sid.clone();
    let t2_c = t2.clone();
    let mut c2_clone = c2.clone();
    let h2 =
        tokio::spawn(async move { wait_for_status(&mut c2_clone, &sid_c, "P2", &t2_c, 10).await });

    let sid_c = sid.clone();
    let t3_c = t3.clone();
    let mut c3_clone = c3.clone();
    let h3 =
        tokio::spawn(async move { wait_for_status(&mut c3_clone, &sid_c, "P3", &t3_c, 10).await });

    // P1 should get your_turn first
    let w1 = h1.await.unwrap();
    assert_eq!(w1.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "P1", &t1, "Position 1").await;

    let w2 = h2.await.unwrap();
    if w2.status == WaitStatus::Waiting as i32 {
        let w2 = wait_for_status(&mut c2, &sid, "P2", &t2, 5).await;
        assert_eq!(w2.status, WaitStatus::YourTurn as i32);
    }
    respond(&mut c2, &sid, "P2", &t2, "Position 2").await;

    let w3 = h3.await.unwrap();
    if w3.status == WaitStatus::Waiting as i32 {
        let w3 = wait_for_status(&mut c3, &sid, "P3", &t3, 5).await;
        assert_eq!(w3.status, WaitStatus::YourTurn as i32);
    }
    respond(&mut c3, &sid, "P3", &t3, "Position 3").await;

    // Vote phase
    let w = wait_for_status(&mut c1, &sid, "P1", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);

    vote(&mut c1, &sid, "P1", &t1, VoteChoice::Yay, "yes").await;
    vote(&mut c2, &sid, "P2", &t2, VoteChoice::Yay, "yes").await;
    vote(&mut c3, &sid, "P3", &t3, VoteChoice::Nay, "no").await;

    let w = wait_for_status(&mut c1, &sid, "P1", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::Complete as i32);
}

// ── Duplicate name rejected ──

#[tokio::test]
async fn test_duplicate_name_rejected() {
    let (_url, mut c1) = start_daemon().await;

    let sid = create_session(&mut c1, "Test?", 1, 3, 30).await;

    join(&mut c1, "Alice", &sid).await;
    let result = c1
        .join(JoinRequest {
            name: "Alice".to_string(),
            session_id: sid.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);
}

// ── Invalid token rejected ──

#[tokio::test]
async fn test_invalid_token_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    let (_, _t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Try with wrong token
    let result = c1
        .wait(WaitRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            timeout_seconds: 1,
            participant_token: "wrong-token".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::PermissionDenied);
}

// ── Join after lobby closed rejected ──

#[tokio::test]
async fn test_join_after_lobby_closed_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    // Two joins meet min_participants, lobby closes
    join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Third join should fail
    let result = c3
        .join(JoinRequest {
            name: "Carol".to_string(),
            session_id: sid.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::FailedPrecondition);
}

// ── Double vote rejected ──

#[tokio::test]
async fn test_double_vote_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // Discussion
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "Alice", &t1, "Pos A").await;

    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "Bob", &t2, "Pos B").await;

    // Vote
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);
    vote(&mut c1, &sid, "Alice", &t1, VoteChoice::Yay, "yes").await;

    // Try to vote again
    let result = c1
        .vote(VoteRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            choice: VoteChoice::Yay as i32,
            reason: "second vote".to_string(),
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);
}

// ── Turn timeout skips unresponsive participant ──

#[tokio::test]
async fn test_turn_timeout_skips_participant() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    // Create session with 1s turn timeout
    let resp = c1
        .create_session(CreateSessionRequest {
            question: "Timeout test?".to_string(),
            rounds: 1,
            min_participants: 2,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 1,
        })
        .await
        .unwrap()
        .into_inner();
    let sid = resp.session_id;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // Alice's turn but she doesn't respond - wait for turn timeout to skip her
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);

    // Wait for turn timeout to fire (1s + buffer)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Bob should now get his turn (Alice was skipped)
    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "Bob", &t2, "Bob's position").await;

    // Vote phase (Alice was skipped, Bob responded - round complete)
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);
}

// ── Name too long rejected ──

#[tokio::test]
async fn test_name_too_long_rejected() {
    let (_url, mut c1) = start_daemon().await;

    let sid = create_session(&mut c1, "Test?", 1, 3, 30).await;

    let long_name = "A".repeat(101);
    let result = c1
        .join(JoinRequest {
            name: long_name,
            session_id: sid.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Respond position too long rejected ──

#[tokio::test]
async fn test_respond_position_too_long_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Alice's turn - submit position > 300 chars
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);

    let long_position = "X".repeat(301);
    let result = c1
        .respond(RespondRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            position: long_position,
            reasoning: vec!["reason".to_string()],
            concerns: vec![],
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Respond too many reasoning items rejected ──

#[tokio::test]
async fn test_respond_too_many_reasoning_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);

    let result = c1
        .respond(RespondRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            position: "Valid position".to_string(),
            reasoning: vec![
                "1".into(),
                "2".into(),
                "3".into(),
                "4".into(),
                "5".into(),
                "6".into(),
            ],
            concerns: vec![],
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Wrong session_id rejected ──

#[tokio::test]
async fn test_wrong_session_id_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Try wait with wrong session ID
    let result = c1
        .wait(WaitRequest {
            session_id: "wrong-session-id".to_string(),
            name: "Alice".to_string(),
            timeout_seconds: 1,
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);

    // Verify correct session ID still works
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 1).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
}

// ── Multi-session isolation ──

#[tokio::test]
async fn test_multi_session_isolation() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c4 = CouncilClient::connect(url).await.unwrap();

    // Create two independent sessions
    let sid1 = create_session(&mut c1, "Session 1 question?", 1, 2, 30).await;
    let sid2 = create_session(&mut c1, "Session 2 question?", 1, 2, 30).await;

    // Join different sessions
    let (_, t1) = join(&mut c1, "Alice", &sid1).await;
    let (_, t2) = join(&mut c2, "Bob", &sid1).await;
    let (_, t3) = join(&mut c3, "Carol", &sid2).await;
    let (_, t4) = join(&mut c4, "Dave", &sid2).await;

    // Session 1: Alice responds
    let w = wait_for_status(&mut c1, &sid1, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    assert_eq!(w.question, "Session 1 question?");
    respond(&mut c1, &sid1, "Alice", &t1, "Session 1 pos").await;

    // Session 2: Carol responds (independent)
    let w = wait_for_status(&mut c3, &sid2, "Carol", &t3, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    assert_eq!(w.question, "Session 2 question?");
    respond(&mut c3, &sid2, "Carol", &t3, "Session 2 pos").await;

    // Cross-session: Alice's token doesn't work in session 2
    let result = c1
        .wait(WaitRequest {
            session_id: sid2.clone(),
            name: "Alice".to_string(),
            timeout_seconds: 1,
            participant_token: t1.clone(),
        })
        .await;
    assert!(result.is_err());

    // Continue session 1 to completion
    let w = wait_for_status(&mut c2, &sid1, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid1, "Bob", &t2, "Bob pos").await;

    let w = wait_for_status(&mut c1, &sid1, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);
    vote(&mut c1, &sid1, "Alice", &t1, VoteChoice::Yay, "yes").await;
    vote(&mut c2, &sid1, "Bob", &t2, VoteChoice::Yay, "yes").await;

    let w = wait_for_status(&mut c1, &sid1, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::Complete as i32);

    // Session 2 should still be in progress
    let w = wait_for_status(&mut c4, &sid2, "Dave", &t4, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
}

// ── GetSession status ──

#[tokio::test]
async fn test_get_session_status() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Status test?", 1, 2, 30).await;

    // Before any joins
    let resp = c1
        .get_session(GetSessionRequest {
            session_id: sid.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.status, council_proto::SessionStatus::LobbyOpen as i32);
    assert!(resp.participants.is_empty());

    // After joins (starts discussion)
    join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    let resp = c1
        .get_session(GetSessionRequest {
            session_id: sid.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.status, council_proto::SessionStatus::InProgress as i32);
    assert_eq!(resp.participants.len(), 2);
    assert_eq!(resp.current_round, 1);
    assert_eq!(resp.total_rounds, 1);
}

// ── ListSessions ──

#[tokio::test]
async fn test_list_sessions() {
    let (_url, mut c1) = start_daemon().await;

    // No sessions initially
    let resp = c1
        .list_sessions(ListSessionsRequest {})
        .await
        .unwrap()
        .into_inner();
    assert!(resp.sessions.is_empty());

    // Create two sessions
    let sid1 = create_session(&mut c1, "Question 1?", 1, 2, 30).await;
    let sid2 = create_session(&mut c1, "Question 2?", 2, 3, 60).await;

    let resp = c1
        .list_sessions(ListSessionsRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.sessions.len(), 2);

    let ids: Vec<&str> = resp.sessions.iter().map(|s| s.session_id.as_str()).collect();
    assert!(ids.contains(&sid1.as_str()));
    assert!(ids.contains(&sid2.as_str()));

    for s in &resp.sessions {
        assert_eq!(s.status, council_proto::SessionStatus::LobbyOpen as i32);
        assert_eq!(s.participant_count, 0);
    }
}

// ── CreateSession validation ──

#[tokio::test]
async fn test_create_session_validation() {
    let (_url, mut c1) = start_daemon().await;

    // Empty question
    let result = c1
        .create_session(CreateSessionRequest {
            question: "".to_string(),
            rounds: 1,
            min_participants: 2,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 120,
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    // Rounds too high
    let result = c1
        .create_session(CreateSessionRequest {
            question: "Valid question?".to_string(),
            rounds: 11,
            min_participants: 2,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 120,
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── CreateSession defaults ──

#[tokio::test]
async fn test_create_session_defaults() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url.clone()).await.unwrap();

    // rounds=0 should default to 2
    let resp = c1
        .create_session(CreateSessionRequest {
            question: "Defaults test?".to_string(),
            rounds: 0,
            min_participants: 2,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 120,
        })
        .await
        .unwrap()
        .into_inner();
    let sid = resp.session_id;

    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // Should have 2 rounds — verify via GetSession
    let info = c1
        .get_session(GetSessionRequest {
            session_id: sid.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(info.total_rounds, 2);

    // Complete round 1
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "Alice", &t1, "Pos A r1").await;
    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "Bob", &t2, "Pos B r1").await;

    // Should still be in progress (round 2), not vote phase
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    assert_eq!(w.current_round, 2);

    // min_participants=0 should default to 3
    let resp = c1
        .create_session(CreateSessionRequest {
            question: "Min participants default?".to_string(),
            rounds: 1,
            min_participants: 0,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 120,
        })
        .await
        .unwrap()
        .into_inner();
    let sid2 = resp.session_id;

    // Join 2 — should NOT auto-start (needs 3)
    join(&mut c1, "X", &sid2).await;
    let (_, t_y) = join(&mut c2, "Y", &sid2).await;
    let w = wait_for_status(&mut c2, &sid2, "Y", &t_y, 1).await;
    assert_eq!(w.status, WaitStatus::Lobby as i32);

    // Join a third — should auto-start
    join(&mut c3, "Z", &sid2).await;
    let w = wait_for_status(&mut c2, &sid2, "Y", &t_y, 5).await;
    assert!(
        w.status == WaitStatus::YourTurn as i32 || w.status == WaitStatus::Waiting as i32,
        "Expected discussion to start, got status: {}",
        w.status
    );
}

// ── Join with empty session_id rejected ──

#[tokio::test]
async fn test_join_empty_session_id_rejected() {
    let (_url, mut c1) = start_daemon().await;

    let result = c1
        .join(JoinRequest {
            name: "Alice".to_string(),
            session_id: "".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    // Whitespace-only should also fail
    let result = c1
        .join(JoinRequest {
            name: "Alice".to_string(),
            session_id: "   ".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Session eviction at max capacity ──

#[tokio::test]
async fn test_session_eviction_at_max_capacity() {
    let (_url, mut c1) = start_daemon().await;

    // Create 10 sessions (the max)
    let mut session_ids = Vec::new();
    for i in 0..10 {
        let sid = create_session(&mut c1, &format!("Question {}?", i), 1, 2, 30).await;
        session_ids.push(sid);
    }

    // All 10 should be accessible
    let resp = c1
        .list_sessions(ListSessionsRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.sessions.len(), 10);

    // Creating an 11th should evict the oldest (session 0)
    let sid_new = create_session(&mut c1, "Question 10?", 1, 2, 30).await;

    let resp = c1
        .list_sessions(ListSessionsRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(resp.sessions.len(), 10);

    // The oldest session should be gone
    let result = c1
        .get_session(GetSessionRequest {
            session_id: session_ids[0].clone(),
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);

    // The second session should still exist
    let result = c1
        .get_session(GetSessionRequest {
            session_id: session_ids[1].clone(),
        })
        .await;
    assert!(result.is_ok());

    // The new session should exist
    let result = c1
        .get_session(GetSessionRequest {
            session_id: sid_new.clone(),
        })
        .await;
    assert!(result.is_ok());

    // Verify ListSessions returns exactly the expected 10 sessions
    let resp = c1
        .list_sessions(ListSessionsRequest {})
        .await
        .unwrap()
        .into_inner();
    let listed_ids: Vec<&str> = resp.sessions.iter().map(|s| s.session_id.as_str()).collect();
    // Sessions 1-9 should remain, session 0 was evicted
    for sid in &session_ids[1..] {
        assert!(
            listed_ids.contains(&sid.as_str()),
            "Expected session {} to still exist",
            sid
        );
    }
    assert!(listed_ids.contains(&sid_new.as_str()));
    assert!(!listed_ids.contains(&session_ids[0].as_str()));
}

// ── Results on non-completed session rejected ──

#[tokio::test]
async fn test_results_before_completion_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;
    join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    // Session is InProgress, not Completed
    let result = c1
        .results(ResultsRequest {
            session_id: sid.clone(),
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::FailedPrecondition);
}

// ── Respond with empty reasoning rejected ──

#[tokio::test]
async fn test_respond_empty_reasoning_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;
    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    join(&mut c2, "Bob", &sid).await;

    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);

    let result = c1
        .respond(RespondRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            position: "Valid position".to_string(),
            reasoning: vec![], // empty
            concerns: vec![],
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Vote with unspecified choice rejected ──

#[tokio::test]
async fn test_vote_unspecified_choice_rejected() {
    let (url, mut c1) = start_daemon().await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let sid = create_session(&mut c1, "Test?", 1, 2, 30).await;
    let (_, t1) = join(&mut c1, "Alice", &sid).await;
    let (_, t2) = join(&mut c2, "Bob", &sid).await;

    // Complete discussion
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c1, &sid, "Alice", &t1, "Pos A").await;
    let w = wait_for_status(&mut c2, &sid, "Bob", &t2, 5).await;
    assert_eq!(w.status, WaitStatus::YourTurn as i32);
    respond(&mut c2, &sid, "Bob", &t2, "Pos B").await;

    // Vote phase — send VOTE_CHOICE_UNSPECIFIED (0)
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 5).await;
    assert_eq!(w.status, WaitStatus::VotePhase as i32);

    let result = c1
        .vote(VoteRequest {
            session_id: sid.clone(),
            name: "Alice".to_string(),
            choice: 0, // VOTE_CHOICE_UNSPECIFIED
            reason: "testing invalid choice".to_string(),
            participant_token: t1.clone(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Wait timeout returns current status ──

#[tokio::test]
async fn test_wait_timeout_returns_status() {
    let (_url, mut c1) = start_daemon().await;

    // Create session but only 1 of 2 required participants joins
    let sid = create_session(&mut c1, "Timeout test?", 1, 2, 60).await;
    let (_, t1) = join(&mut c1, "Alice", &sid).await;

    // Wait with a 1-second timeout — should return lobby status, not block
    let w = wait_for_status(&mut c1, &sid, "Alice", &t1, 1).await;
    assert_eq!(w.status, WaitStatus::Lobby as i32);
}

// ── Lobby timeout with zero participants removes session ──

#[tokio::test]
async fn test_lobby_timeout_zero_participants_removes_session() {
    let (_url, mut c1) = start_daemon().await;

    // Create session with a 1-second join timeout, but don't join anyone
    let resp = c1
        .create_session(CreateSessionRequest {
            question: "Ghost session?".to_string(),
            rounds: 1,
            min_participants: 2,
            join_timeout_seconds: 1,
            turn_timeout_seconds: 120,
        })
        .await
        .unwrap()
        .into_inner();
    let sid = resp.session_id;

    // Session exists right after creation
    let result = c1
        .get_session(GetSessionRequest {
            session_id: sid.clone(),
        })
        .await;
    assert!(result.is_ok());

    // Wait for lobby timeout to fire and clean up (1s + buffer)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Session should have been removed
    let result = c1
        .get_session(GetSessionRequest {
            session_id: sid.clone(),
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::NotFound);
}

// ── Config upper bounds validation ──

#[tokio::test]
async fn test_create_session_config_upper_bounds() {
    let (_url, mut c1) = start_daemon().await;

    // min_participants too high
    let result = c1
        .create_session(CreateSessionRequest {
            question: "Test?".to_string(),
            rounds: 1,
            min_participants: 51,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 120,
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    // join_timeout too high
    let result = c1
        .create_session(CreateSessionRequest {
            question: "Test?".to_string(),
            rounds: 1,
            min_participants: 2,
            join_timeout_seconds: 3601,
            turn_timeout_seconds: 120,
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);

    // turn_timeout too high
    let result = c1
        .create_session(CreateSessionRequest {
            question: "Test?".to_string(),
            rounds: 1,
            min_participants: 2,
            join_timeout_seconds: 30,
            turn_timeout_seconds: 3601,
        })
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}
