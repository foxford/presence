use crate::classroom::ClassroomId;
use sqlx::postgres::PgTypeInfo;
use std::fmt::{Display, Formatter};
use svc_agent::AgentId;
use uuid::Uuid;

#[derive(Hash, Eq, PartialEq, Clone, sqlx::Type, Debug)]
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

#[derive(Debug, sqlx::Type, Clone, Copy)]
#[sqlx(transparent)]
pub struct SessionId(Uuid);

impl sqlx::postgres::PgHasArrayType for SessionId {
    fn array_type_info() -> PgTypeInfo {
        <Uuid as sqlx::postgres::PgHasArrayType>::array_type_info()
    }
}

impl From<Uuid> for SessionId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}
