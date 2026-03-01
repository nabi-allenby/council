use std::time::Duration;

use council_daemon::config::DaemonConfig;
use council_daemon::server::CouncilService;
use council_proto::council_client::CouncilClient;
use council_proto::council_server::CouncilServer;
use council_proto::*;
use tokio::net::TcpListener;
use tonic::transport::Channel;
use tonic::transport::Server;

/// Helper: start a daemon on a random port and return the address + a client.
async fn start_daemon(
    question: &str,
    rounds: u32,
    min_participants: u32,
    join_timeout_secs: u64,
) -> (String, CouncilClient<Channel>) {
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let config = DaemonConfig {
        rounds,
        min_participants,
        join_timeout: Duration::from_secs(join_timeout_secs),
        turn_timeout: Duration::from_secs(120),
    };
    let service = CouncilService::new(question.to_string(), config);
    service.spawn_lobby_timeout();

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

/// Helper: join a participant and return (session_id, token)
async fn join(client: &mut CouncilClient<Channel>, name: &str) -> (String, String) {
    let resp = client
        .join(JoinRequest {
            name: name.to_string(),
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
    let (url, mut c1) = start_daemon("Should we adopt Rust?", 1, 3, 30).await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    // Join
    let (sid, t1) = join(&mut c1, "Alice").await;
    let (_, t2) = join(&mut c2, "Bob").await;
    let (_, t3) = join(&mut c3, "Carol").await;

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
    let (url, mut c1) = start_daemon("Test question?", 1, 5, 1).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    let (_, t2) = join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Test question?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, _t1) = join(&mut c1, "Alice").await;
    let (_, t2) = join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Test question?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Should we merge?", 1, 3, 30).await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "A").await;
    let (_, t2) = join(&mut c2, "B").await;
    let (_, t3) = join(&mut c3, "C").await;

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
    let (url, mut c1) = start_daemon("Concurrent test?", 1, 3, 30).await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "P1").await;
    let (_, t2) = join(&mut c2, "P2").await;
    let (_, t3) = join(&mut c3, "P3").await;

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
    let (_url, mut c1) = start_daemon("Test?", 1, 3, 30).await;

    join(&mut c1, "Alice").await;
    let result = c1
        .join(JoinRequest {
            name: "Alice".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::AlreadyExists);
}

// ── Invalid token rejected ──

#[tokio::test]
async fn test_invalid_token_rejected() {
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, _t1) = join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c3 = CouncilClient::connect(url).await.unwrap();

    // Two joins meet min_participants, lobby closes
    join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

    // Third join should fail
    let result = c3
        .join(JoinRequest {
            name: "Carol".to_string(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::FailedPrecondition);
}

// ── Double vote rejected ──

#[tokio::test]
async fn test_double_vote_rejected() {
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    let (_, t2) = join(&mut c2, "Bob").await;

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
    // Use a very short turn timeout (1s)
    let listener = TcpListener::bind("[::1]:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}", addr);

    let config = DaemonConfig {
        rounds: 1,
        min_participants: 2,
        join_timeout: Duration::from_secs(30),
        turn_timeout: Duration::from_secs(1),
    };
    let service = CouncilService::new("Timeout test?".to_string(), config);
    service.spawn_lobby_timeout();

    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    tokio::spawn(async move {
        Server::builder()
            .add_service(CouncilServer::new(service))
            .serve_with_incoming(incoming)
            .await
            .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut c1 = CouncilClient::connect(url.clone()).await.unwrap();
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    let (_, t2) = join(&mut c2, "Bob").await;

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
    let (_url, mut c1) = start_daemon("Test?", 1, 3, 30).await;

    let long_name = "A".repeat(101);
    let result = c1.join(JoinRequest { name: long_name }).await;

    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code(), tonic::Code::InvalidArgument);
}

// ── Respond position too long rejected ──

#[tokio::test]
async fn test_respond_position_too_long_rejected() {
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

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
    let (url, mut c1) = start_daemon("Test?", 1, 2, 30).await;
    let mut c2 = CouncilClient::connect(url).await.unwrap();

    let (sid, t1) = join(&mut c1, "Alice").await;
    join(&mut c2, "Bob").await;

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
