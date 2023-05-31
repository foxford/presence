use crate::{
    classroom::ClassroomId,
    session::{SessionId, SessionKey},
};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use svc_events::EventV1 as Event;
use svc_nats_client::{AckPolicy, DeliverPolicy, EventId, Message, NatsClient as AsyncNatsClient};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::{interval, Duration as TokioDuration, Interval, MissedTickBehavior},
};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

const SUBJECT_PREFIX: &str = "classroom";
const ENTITY_TYPE: &str = "agent";

#[derive(Debug)]
struct Subscribe {
    classroom_id: ClassroomId,
    resp_chan: oneshot::Sender<Result<broadcast::Receiver<Message>>>,
}

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(600);
const CLEANUP_PERIOD: TokioDuration = TokioDuration::from_secs(60);

#[derive(Debug)]
enum Cmd {
    Cleanup,
    Subscribe(Subscribe),
    Shutdown,
}

#[derive(Clone)]
pub struct Client {
    tx: mpsc::UnboundedSender<Subscribe>,
    shutdown_tx: mpsc::Sender<oneshot::Sender<()>>,
    inner: svc_nats_client::Client,
}

#[async_trait]
pub trait NatsClient: Send + Sync {
    async fn subscribe(&self, classroom_id: ClassroomId) -> Result<broadcast::Receiver<Message>>;
    async fn publish_event(
        &self,
        session_key: SessionKey,
        session_id: SessionId,
        event: Event,
    ) -> Result<()>;
}

impl Client {
    pub async fn new(cfg: svc_nats_client::Config) -> Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Subscribe>();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<oneshot::Sender<()>>(1);

        let nats_client = svc_nats_client::Client::new(cfg).await?;
        info!("Connected to NATS");

        let client = nats_client.clone();
        tokio::spawn(async move {
            let (inner_tx, inner_rx) = mpsc::unbounded_channel();

            let join_handle = tokio::spawn(nats_loop(client, inner_rx));

            let mut cleanup_interval = cleanup_interval(CLEANUP_PERIOD);

            loop {
                tokio::select! {
                    Some(wait_tx) = shutdown_rx.recv() => {
                        info!("Nats client received shutdown");

                        let _ = inner_tx.send(Cmd::Shutdown);
                        let r = join_handle.await;
                        if let Err(e) = r {
                            warn!(error = ?e, "Failed to shutdown nats client thread properly");
                        }
                        if wait_tx.send(()).is_err() {
                            warn!("Failed to notify of nats client thread shutdown");
                        }
                        return;
                    }
                    Some(sub) = rx.recv() => {
                        let classroom_id = sub.classroom_id;
                        info!(%classroom_id, "Nats client received subscribe");

                        let _ = inner_tx.send(Cmd::Subscribe(sub));
                    }
                    _ = cleanup_interval.tick() => {
                        let _ = inner_tx.send(Cmd::Cleanup);
                    }
                }
            }
        });

        Ok(Self {
            tx,
            shutdown_tx,
            inner: nats_client,
        })
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (wait_tx, wait_rx) = oneshot::channel();
        self.shutdown_tx
            .send(wait_tx)
            .await
            .context("Failed to send shutdown to nats task")?;
        wait_rx.await.context("Failed to await nats shutdown")
    }
}

#[async_trait]
impl NatsClient for Client {
    async fn subscribe(&self, classroom_id: ClassroomId) -> Result<broadcast::Receiver<Message>> {
        let (resp_chan, resp_rx) = oneshot::channel();
        self.tx
            .send(Subscribe {
                classroom_id,
                resp_chan,
            })
            .context("Failed to send cmd to nats loop")?;
        resp_rx
            .await
            .context("Failed to await receive from nats loop")?
            .context("Subscription failed")
    }

    async fn publish_event(
        &self,
        session_key: SessionKey,
        session_id: SessionId,
        event: Event,
    ) -> Result<()> {
        let subject = svc_nats_client::Subject::new(
            SUBJECT_PREFIX.to_string(),
            session_key.classroom_id.into(),
            ENTITY_TYPE.to_string(),
        );

        let event = svc_events::Event::from(event);
        let payload = serde_json::to_vec(&event)?;

        let event_id = EventId::from((ENTITY_TYPE.to_string(), session_id.into()));

        let event =
            svc_nats_client::event::Builder::new(subject, payload, event_id, session_key.agent_id)
                .internal(false)
                .deduplication(false)
                .build();

        self.inner.publish(&event).await?;

        Ok(())
    }
}

async fn nats_loop(client: svc_nats_client::Client, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    let mut subscribers = Subscriptions::new();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Subscribe(Subscribe {
                classroom_id,
                resp_chan,
            }) => {
                let maybe_rx = subscribers.get(&classroom_id).and_then(|sub| {
                    if sub.tx.receiver_count() > 0 {
                        Some(sub.tx.subscribe())
                    } else {
                        None
                    }
                });
                let resp_result = match maybe_rx {
                    Some(rx) => {
                        info!("Sending existing sub tx");
                        resp_chan.send(Ok(rx))
                    }
                    None => {
                        let sub =
                            add_new_subscription(&client, &mut subscribers, classroom_id).await;
                        resp_chan.send(sub)
                    }
                };

                if resp_result.is_err() {
                    warn!(
                        %classroom_id,
                        "Failed to send nats subscription channel back"
                    );
                }
            }
            Cmd::Shutdown => break,
            Cmd::Cleanup => subscribers.cleanup(CLEANUP_TIMEOUT),
        }
    }
}

async fn add_new_subscription(
    client: &svc_nats_client::Client,
    subscribers: &mut Subscriptions,
    classroom_id: ClassroomId,
) -> Result<broadcast::Receiver<Message>> {
    let subject = svc_nats_client::Subject::new(
        SUBJECT_PREFIX.to_string(),
        classroom_id.into(),
        "*".to_string(),
    );

    info!(%subject, "Subscribing to JetStream");

    let mut messages = client
        .subscribe_ephemeral(subject.clone(), DeliverPolicy::New, AckPolicy::None)
        .await
        .map_err(|err| {
            error!(%classroom_id, %err, "failed to create an ephemeral subscription");

            let error = anyhow!("failed to create an ephemeral subscription, error: {}", err);
            if let Err(err) = svc_error::extension::sentry::send(Arc::new(error)) {
                error!(%err, "failed to send error to sentry");
            }

            err
        })?;

    info!(%subject, "Subscribed to JetStream");

    let (tx, rx) = broadcast::channel(50);
    let tx_ = tx.clone();

    tokio::spawn(async move {
        while let Some(Ok(message)) = messages.next().await {
            if tx_.send(message).is_err() {
                error!(%classroom_id, "Failed to send message from nats subscription to receiver");
                return;
            }
        }
    });

    subscribers.insert(classroom_id, tx);
    Ok(rx)
}

struct Subscriptions {
    map: HashMap<ClassroomId, Subscription>,
}

impl Subscriptions {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn insert(&mut self, cid: ClassroomId, tx: broadcast::Sender<Message>) -> Option<Subscription> {
        self.map.insert(
            cid,
            Subscription {
                tx,
                created_at: Instant::now(),
            },
        )
    }

    fn get(&self, cid: &ClassroomId) -> Option<&Subscription> {
        self.map.get(cid)
    }

    fn cleanup(&mut self, timeout: Duration) {
        self.map
            .retain(|_cid, sub| sub.created_at.elapsed() < timeout || sub.tx.receiver_count() > 0)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.map.len()
    }
}

struct Subscription {
    tx: broadcast::Sender<Message>,
    created_at: Instant,
}

fn cleanup_interval(d: TokioDuration) -> Interval {
    let mut interval = interval(d);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    interval
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_subscribers_cleanup() {
        let mut subscribers = Subscriptions::new();
        let (tx, rx) = broadcast::channel(50);
        drop(rx);

        subscribers.insert(Uuid::new_v4().into(), tx);
        assert_eq!(subscribers.len(), 1);

        tokio::time::sleep(TokioDuration::from_millis(500)).await;

        subscribers.cleanup(Duration::from_millis(300));
        assert_eq!(subscribers.len(), 0);
    }
}
