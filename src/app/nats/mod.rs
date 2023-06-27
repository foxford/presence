use crate::{classroom::ClassroomId, session::Session};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::sync::Arc;
use svc_events::{EventId, EventV1 as Event};
use svc_nats_client::{AckPolicy, DeliverPolicy, Message, NatsClient as AsyncNatsClient};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

const SUBJECT_PREFIX: &str = "classroom";
const ENTITY_TYPE: &str = "agent";

#[derive(Debug)]
struct Subscribe {
    classroom_id: ClassroomId,
    resp_chan: oneshot::Sender<Result<mpsc::Receiver<Message>>>,
}

#[derive(Debug)]
enum Cmd {
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
    async fn subscribe(&self, classroom_id: ClassroomId) -> Result<mpsc::Receiver<Message>>;
    async fn publish_event(&self, session: &Session, event: Event, operation: String)
        -> Result<()>;
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
    async fn subscribe(&self, classroom_id: ClassroomId) -> Result<mpsc::Receiver<Message>> {
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
        session: &Session,
        event: Event,
        operation: String,
    ) -> Result<()> {
        let subject = svc_nats_client::Subject::new(
            SUBJECT_PREFIX.to_string(),
            session.key().classroom_id.into(),
            ENTITY_TYPE.to_string(),
        );

        let event = svc_events::Event::from(event);
        let payload = serde_json::to_vec(&event)?;

        let event_id = EventId::from((ENTITY_TYPE.to_string(), operation, session.id().into()));

        let event = svc_nats_client::event::Builder::new(
            subject,
            payload,
            event_id,
            session.key().clone().agent_id,
        )
        .internal(false)
        .disable_deduplication()
        .build();

        self.inner.publish(&event).await?;

        Ok(())
    }
}

async fn nats_loop(client: svc_nats_client::Client, mut rx: mpsc::UnboundedReceiver<Cmd>) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Subscribe(Subscribe {
                classroom_id,
                resp_chan,
            }) => {
                let sub = add_new_subscription(&client, classroom_id).await;
                resp_chan.send(sub).ok();
            }
            Cmd::Shutdown => break,
        }
    }
}

async fn add_new_subscription(
    client: &svc_nats_client::Client,
    classroom_id: ClassroomId,
) -> Result<mpsc::Receiver<Message>> {
    let subject = svc_nats_client::Subject::new(
        SUBJECT_PREFIX.to_string(),
        classroom_id.into(),
        "*".to_string(),
    );

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

    let (tx, rx) = mpsc::channel(50);

    tokio::spawn(async move {
        while let Some(Ok(message)) = messages.next().await {
            if tx.send(message).await.is_err() {
                error!(%classroom_id, "Failed to send message from nats subscription to receiver");
                return;
            }
        }
    });

    Ok(rx)
}
