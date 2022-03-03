use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize, sqlx::Type, Copy, Clone, Hash, Eq, PartialEq)]
#[sqlx(transparent)]
pub struct ClassroomId(pub Uuid);
