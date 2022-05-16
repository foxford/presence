use crate::{app::ws::event::Event, classroom::ClassroomId, session::SessionKey};
use anyhow::{Context, Result};
use async_trait::async_trait;
use crossbeam_channel::{unbounded, Receiver};
use nats::{
    jetstream::{JetStream, SubscribeOptions},
    HeaderMap, Message,
};
use std::{
    collections::HashMap,
    io, thread,
    time::{Duration, Instant},
};
use tokio::{
    sync::{broadcast, mpsc, oneshot},
    time::{interval, Duration as tokioDuration, Interval, MissedTickBehavior},
};
use tracing::{error, info, warn};

pub const PRESENCE_EVENT_LABEL: &str = "presence-label";
pub const PRESENCE_SENDER_AGENT_ID: &str = "presence-sender-agent-id";

#[derive(Debug)]
struct Subscribe {
    classroom_id: ClassroomId,
    resp_chan: oneshot::Sender<io::Result<broadcast::Receiver<Message>>>,
}

const CLEANUP_TIMEOUT: Duration = Duration::from_secs(600);
const CLEANUP_PERIOD: tokioDuration = tokioDuration::from_secs(60);

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
    jetstream: JetStream,
}

#[async_trait]
pub trait NatsClient: Send + Sync {
    async fn subscribe(&self, _: ClassroomId) -> Result<broadcast::Receiver<Message>>;
    fn publish_event(&self, _: SessionKey, _: Event) -> Result<()> {
        Ok(())
    }
}

impl Client {
    pub fn new(nats_url: &str) -> Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Subscribe>();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<oneshot::Sender<()>>(1);

        let connection = nats::Options::with_credentials("nats.creds")
            .error_callback(|e| {
                error!(error = ?e, "Nats server error");
            })
            .connect(nats_url)?;

        info!("Connected to nats");
        let jetstream = nats::jetstream::new(connection);

        let js = jetstream.clone();
        tokio::task::spawn(async move {
            let (inner_tx, inner_rx) = unbounded();

            let join_handle = thread::spawn(move || nats_loop(&js, inner_rx));

            let mut cleanup_interval = cleanup_interval(CLEANUP_PERIOD);

            loop {
                tokio::select! {
                    Some(wait_tx) = shutdown_rx.recv() => {
                        info!("Nats client received shutdown");

                        let _ = inner_tx.try_send(Cmd::Shutdown);
                        let r = tokio::task::spawn_blocking(move || join_handle.join()).await;
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

                        let _ = inner_tx.try_send(Cmd::Subscribe(sub));
                    }
                    _ = cleanup_interval.tick() => {
                        let _ = inner_tx.try_send(Cmd::Cleanup);
                    }
                }
            }
        });

        Ok(Self {
            tx,
            shutdown_tx,
            jetstream,
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

    fn publish_event(&self, session_key: SessionKey, event: Event) -> Result<()> {
        let data = serde_json::to_string(&event)?;
        let subject = format!("classrooms.{}.presence", session_key.classroom_id);
        let mut headers = HeaderMap::new();
        headers.insert(PRESENCE_SENDER_AGENT_ID, session_key.agent_id.to_string());
        headers.insert(PRESENCE_EVENT_LABEL, event.label.to_string());

        let msg = Message::new(&subject, None, data, Some(headers));
        self.jetstream.publish_message(&msg)?;

        Ok(())
    }
}

fn nats_loop(js: &JetStream, rx: Receiver<Cmd>) {
    let mut subscribers = Subscriptions::new();

    while let Ok(cmd) = rx.recv() {
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
                        resp_chan.send(add_new_subscription(js, &mut subscribers, classroom_id))
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

fn add_new_subscription(
    js: &JetStream,
    subscribers: &mut Subscriptions,
    classroom_id: ClassroomId,
) -> io::Result<broadcast::Receiver<Message>> {
    let topic = format!("classrooms.{}.*", classroom_id);
    let options = SubscribeOptions::bind_stream("classrooms-reliable".into())
        .deliver_new()
        .ack_none()
        .replay_instant();

    info!(%topic, "Subscribing to jetstream");

    let subscription = js.subscribe_with_options(&topic, &options)?;

    info!(%topic, "Subscribed to jetstream");

    let (tx, rx) = broadcast::channel(50);

    let tx_ = tx.clone();

    std::thread::spawn(move || {
        for message in subscription.iter() {
            if let Err(_e) = tx_.send(message) {
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
    tx: broadcast::Sender<nats::Message>,
    created_at: Instant,
}

fn cleanup_interval(d: tokioDuration) -> Interval {
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

        tokio::time::sleep(tokioDuration::from_millis(500)).await;

        subscribers.cleanup(Duration::from_millis(300));
        assert_eq!(subscribers.len(), 0);
    }
}
