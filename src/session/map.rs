use crate::{
    app::session_manager::ConnectionCommand,
    session::{SessionId, SessionKey},
};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

pub type SessionValue = (SessionId, oneshot::Sender<ConnectionCommand>);

#[derive(Clone, Debug)]
pub struct SessionMap {
    inner: Arc<Mutex<HashMap<SessionKey, SessionValue>>>,
}

impl SessionMap {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Default::default())),
        }
    }

    pub fn insert(&self, key: SessionKey, value: SessionValue) -> Option<SessionValue> {
        let mut map = self.inner.lock().ok()?;
        map.insert(key, value)
    }

    pub fn remove(&self, key: &SessionKey) -> Option<SessionValue> {
        let mut map = self.inner.lock().ok()?;
        map.remove(key)
    }
}
