use sqlx::postgres::PgQueryResult;
use sqlx::types::ipnetwork::IpNetwork;
use sqlx::PgConnection;
use std::net::IpAddr;
use uuid::Uuid;

pub struct InsertQuery {
    label: String,
    ip: IpNetwork,
}

pub struct Replica {
    pub id: Uuid,
}

impl InsertQuery {
    pub fn new(label: String, ip: IpAddr) -> anyhow::Result<Self> {
        Ok(Self {
            label,
            ip: IpNetwork::new(ip, 24)?,
        })
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<Replica> {
        sqlx::query_as!(
            Replica,
            r#"
            INSERT INTO replica
                (label, ip)
            VALUES ($1, $2)
            RETURNING
                id
            "#,
            &self.label,
            &self.ip
        )
        .fetch_one(conn)
        .await
    }
}

pub struct DeleteQuery {
    id: Uuid,
}

impl DeleteQuery {
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<PgQueryResult> {
        sqlx::query!(
            r#"
            DELETE FROM replica
            WHERE id = $1
            "#,
            self.id
        )
        .execute(conn)
        .await
    }
}

pub struct ReplicaIp {
    pub ip: IpNetwork,
}

pub struct GetIpQuery {
    id: Uuid,
}

impl GetIpQuery {
    pub fn new(id: Uuid) -> Self {
        Self { id }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<ReplicaIp> {
        sqlx::query_as!(
            ReplicaIp,
            r#"
            SELECT ip
            FROM replica
            WHERE id = $1
            "#,
            self.id
        )
        .fetch_one(conn)
        .await
    }
}
