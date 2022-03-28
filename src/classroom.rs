use serde_derive::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Deserialize, Serialize, sqlx::Type, Copy, Clone, Hash, Eq, PartialEq, Debug)]
#[sqlx(transparent)]
pub struct ClassroomId(pub Uuid);

impl Display for ClassroomId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
