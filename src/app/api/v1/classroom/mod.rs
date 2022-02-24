use crate::app::api::AppResult;
use crate::app::error::{ErrorExt, ErrorKind};
use crate::classroom::ClassroomId;
use crate::db::agent_session::AgentList;
use crate::state::State;
use anyhow::Context;
use axum::body;
use axum::extract::{Extension, Path};
use http::Response;
use svc_agent::AgentId;
use svc_utils::extractors::AuthnExtractor;

pub async fn list_agents(
    Extension(state): Extension<State>,
    Path(classroom_id): Path<ClassroomId>,
    AuthnExtractor(agent_id): AuthnExtractor,
) -> AppResult {
    do_list_agents(state, classroom_id, agent_id).await
}

async fn do_list_agents(state: State, classroom_id: ClassroomId, _agent_id: AgentId) -> AppResult {
    let mut conn = state
        .get_conn()
        .await
        .error(ErrorKind::DbConnAcquisitionFailed)?;

    let agents = AgentList::new(classroom_id)
        .execute(&mut conn)
        .await
        .context("Failed to get list of agents")
        .error(ErrorKind::DbQueryFailed)?;

    let body = serde_json::to_string(&agents)
        .context("Failed to serialize agents")
        .error(ErrorKind::SerializationFailed)?;

    let resp = Response::builder()
        .body(body::boxed(body::Full::from(body)))
        .context("Failed to build response for agents")
        .error(ErrorKind::ResponseBuildFailed)?;

    Ok(resp)
}
