use serde_derive::{Deserialize, Serialize};
use sqlx::postgres::PgTypeInfo;
use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Deserialize, Serialize, sqlx::Type, Copy, Clone, Hash, Eq, PartialEq, Debug)]
#[sqlx(transparent)]
pub struct ClassroomId(Uuid);

impl Display for ClassroomId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl sqlx::postgres::PgHasArrayType for ClassroomId {
    fn array_type_info() -> PgTypeInfo {
        <Uuid as sqlx::postgres::PgHasArrayType>::array_type_info()
    }
}

impl From<Uuid> for ClassroomId {
    fn from(value: Uuid) -> Self {
        Self(value)
    }
}

impl From<ClassroomId> for Uuid {
    fn from(value: ClassroomId) -> Self {
        value.0
    }
}
