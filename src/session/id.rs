use sqlx::postgres::PgTypeInfo;
use uuid::Uuid;

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
