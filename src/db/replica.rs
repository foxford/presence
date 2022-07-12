use crate::{classroom::ClassroomId, session::SessionKey};
use sqlx::postgres::PgQueryResult;
use sqlx::types::ipnetwork::IpNetwork;
use sqlx::PgConnection;
use std::net::IpAddr;
use svc_agent::AgentId;
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
            INSERT INTO replica (label, ip)
            VALUES ($1, $2)
            ON CONFLICT (label)
            DO UPDATE SET ip = EXCLUDED.ip
            RETURNING id
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
    ip: IpNetwork,
}

impl ReplicaIp {
    pub fn ip(&self) -> IpAddr {
        self.ip.ip()
    }
}

pub struct GetIpQuery {
    session_key: SessionKey,
}

impl GetIpQuery {
    pub fn new(session_key: SessionKey) -> Self {
        Self { session_key }
    }

    pub async fn execute(&self, conn: &mut PgConnection) -> sqlx::Result<ReplicaIp> {
        sqlx::query_as!(
            ReplicaIp,
            r#"
            SELECT replica.ip
            FROM replica
            JOIN agent_session
                ON replica.id = agent_session.replica_id
            WHERE agent_session.agent_id = $1
                AND agent_session.classroom_id = $2
            LIMIT 1
            "#,
            &self.session_key.agent_id as &AgentId,
            &self.session_key.classroom_id as &ClassroomId,
        )
        .fetch_one(conn)
        .await
    }
}
