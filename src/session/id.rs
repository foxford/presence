use serde::{Deserialize, Serialize};
use sqlx::postgres::PgTypeInfo;
use std::fmt::{Display, Formatter};

#[derive(Debug, sqlx::Type, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[sqlx(transparent)]
pub struct SessionId(i64);

impl sqlx::postgres::PgHasArrayType for SessionId {
    fn array_type_info() -> PgTypeInfo {
        <i64 as sqlx::postgres::PgHasArrayType>::array_type_info()
    }
}

impl From<i64> for SessionId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl From<SessionId> for i64 {
    fn from(value: SessionId) -> Self {
        value.0
    }
}

impl Display for SessionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
