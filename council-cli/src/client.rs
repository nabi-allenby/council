use council_proto::council_client::CouncilClient;
use council_proto::*;
use tonic::transport::Channel;

use crate::error::CliError;

pub async fn connect(addr: &str) -> Result<CouncilClient<Channel>, CliError> {
    let url = if addr.starts_with("http") {
        addr.to_string()
    } else {
        format!("http://{}", addr)
    };
    let client = CouncilClient::connect(url).await?;
    Ok(client)
}

pub async fn get_session(addr: &str, session_id: &str) -> Result<GetSessionResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .get_session(GetSessionRequest {
            session_id: session_id.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn list_sessions(addr: &str) -> Result<ListSessionsResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .list_sessions(ListSessionsRequest {})
        .await?;
    Ok(response.into_inner())
}

pub async fn join(addr: &str, name: &str, session_id: &str) -> Result<JoinResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .join(JoinRequest {
            name: name.to_string(),
            session_id: session_id.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn wait(
    addr: &str,
    session_id: &str,
    name: &str,
    token: &str,
    timeout_seconds: u32,
) -> Result<WaitResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .wait(WaitRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            timeout_seconds,
            participant_token: token.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn respond(
    addr: &str,
    session_id: &str,
    name: &str,
    token: &str,
    position: &str,
    reasoning: Vec<String>,
    concerns: Vec<String>,
) -> Result<RespondResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .respond(RespondRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            position: position.to_string(),
            reasoning,
            concerns,
            participant_token: token.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn vote(
    addr: &str,
    session_id: &str,
    name: &str,
    token: &str,
    choice: &str,
    reason: &str,
) -> Result<VoteResponse, CliError> {
    let vote_choice = match choice.to_lowercase().as_str() {
        "yay" | "yes" | "y" => council_proto::VoteChoice::Yay,
        "nay" | "no" | "n" => council_proto::VoteChoice::Nay,
        _ => return Err(CliError::Rpc("choice must be 'yay' or 'nay'".to_string())),
    };

    let mut client = connect(addr).await?;
    let response = client
        .vote(VoteRequest {
            session_id: session_id.to_string(),
            name: name.to_string(),
            choice: vote_choice as i32,
            reason: reason.to_string(),
            participant_token: token.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}

pub async fn results(addr: &str, session_id: &str) -> Result<ResultsResponse, CliError> {
    let mut client = connect(addr).await?;
    let response = client
        .results(ResultsRequest {
            session_id: session_id.to_string(),
        })
        .await?;
    Ok(response.into_inner())
}
