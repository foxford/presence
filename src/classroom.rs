use serde_derive::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize, sqlx::Type, Copy, Clone)]
#[sqlx(transparent)]
pub struct ClassroomId(pub Uuid);
