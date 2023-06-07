mod id;
mod key;
mod kind;

pub use id::SessionId;
pub use key::SessionKey;
pub use kind::SessionKind;

#[derive(Debug)]
pub struct Session {
    id: SessionId,
    key: SessionKey,
    kind: SessionKind,
}

impl Session {
    pub fn new(id: SessionId, key: SessionKey, kind: SessionKind) -> Self {
        Self { id, key, kind }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn key(&self) -> &SessionKey {
        &self.key
    }

    pub fn kind(&self) -> SessionKind {
        self.kind
    }
}
