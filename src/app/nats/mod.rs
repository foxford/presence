use std::{collections::HashMap, io, thread};

use anyhow::Context;
use crossbeam_channel::{unbounded, Receiver};
use nats::{
    jetstream::{JetStream, SubscribeOptions},
    Message,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::warn;

use crate::classroom::ClassroomId;

#[derive(Debug)]
struct Subscribe {
    classroom_id: ClassroomId,
    resp_chan: oneshot::Sender<io::Result<broadcast::Receiver<Message>>>,
}

#[derive(Debug)]
enum Cmd {
    Subscribe(Subscribe),
    Shutdown,
}

#[derive(Clone)]
pub struct NatsClient {
    tx: mpsc::UnboundedSender<Subscribe>,
    shutdown_tx: mpsc::Sender<oneshot::Sender<()>>,
}

impl NatsClient {
    pub fn new(nats_url: &str) -> anyhow::Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<oneshot::Sender<()>>(1);

        let connection = nats::Options::with_credentials("nats.creds").connect(nats_url)?;
        let jetstream = nats::jetstream::new(connection);

        tokio::task::spawn(async move {
            let (inner_tx, inner_rx) = unbounded();

            let join_handle = thread::spawn(move || nats_loop(jetstream, inner_rx));

            loop {
                tokio::select! {
                    Some(wait_tx) = shutdown_rx.recv() => {
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
                        let _ = inner_tx.try_send(Cmd::Subscribe(sub));
                    }
                }
            }
        });

        Ok(Self { tx, shutdown_tx })
    }

    pub async fn subscribe(
        &self,
        classroom_id: ClassroomId,
    ) -> Result<broadcast::Receiver<Message>, anyhow::Error> {
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

    pub async fn shutdown(&self) -> Result<(), anyhow::Error> {
        let (wait_tx, wait_rx) = oneshot::channel();
        self.shutdown_tx
            .send(wait_tx)
            .await
            .context("Failed to send shutdown to nats task")?;
        wait_rx.await.context("Failed to await nats shutdown")
    }
}

fn nats_loop(js: JetStream, rx: Receiver<Cmd>) {
    let mut subscribers: HashMap<ClassroomId, Subscription> = HashMap::new();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Subscribe(Subscribe {
                classroom_id,
                resp_chan,
            }) => {
                let resp_result = if let Some(sub) = subscribers.get(&classroom_id) {
                    resp_chan.send(Ok(sub.tx.subscribe()))
                } else {
                    resp_chan.send(add_new_subscription(&js, &mut subscribers, classroom_id))
                };

                if resp_result.is_err() {
                    warn!(
                        %classroom_id,
                        "Failed to send nats subscription channel back"
                    );
                }
            }
            Cmd::Shutdown => break,
        }
    }
}

fn add_new_subscription(
    js: &JetStream,
    subscribers: &mut HashMap<ClassroomId, Subscription>,
    classroom_id: ClassroomId,
) -> io::Result<broadcast::Receiver<Message>> {
    let topic = format!("classrooms.{}.*", classroom_id);
    let options = SubscribeOptions::bind_stream("classrooms-reliable".into())
        .deliver_last()
        .ack_none()
        .replay_instant();
    let subscription = js.subscribe_with_options(&topic, &options)?;
    let (tx, rx) = broadcast::channel(50);

    let tx_ = tx.clone();

    std::thread::spawn(move || {
        for message in subscription.iter() {
            if let Err(_e) = tx_.send(message) {
                return;
            }
        }
    });

    subscribers.insert(classroom_id, Subscription { tx });
    Ok(rx)
}

struct Subscription {
    tx: broadcast::Sender<nats::Message>,
}
