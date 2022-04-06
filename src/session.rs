use crate::classroom::ClassroomId;
use std::fmt::{Display, Formatter};
use svc_agent::AgentId;

#[derive(Hash, Eq, PartialEq, Clone, sqlx::Type, Debug)]
pub struct SessionKey((AgentId, ClassroomId));

impl SessionKey {
    pub fn new(agent_id: AgentId, classroom_id: ClassroomId) -> Self {
        Self((agent_id, classroom_id))
    }
}

impl Display for SessionKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.0 .0, self.0 .1)
    }
}
