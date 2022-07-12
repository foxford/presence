use crate::classroom::ClassroomId;
use serde_derive::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use svc_agent::AgentId;

#[derive(Hash, Eq, PartialEq, Clone, sqlx::Type, Debug, Deserialize, Serialize)]
pub struct SessionKey {
    pub agent_id: AgentId,
    pub classroom_id: ClassroomId,
}

impl SessionKey {
    pub fn new(agent_id: AgentId, classroom_id: ClassroomId) -> Self {
        Self {
            agent_id,
            classroom_id,
        }
    }
}

impl Display for SessionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.agent_id, self.classroom_id)
    }
}
