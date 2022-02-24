use std::{collections::HashMap, io, thread};

use anyhow::Context;
use crossbeam_channel::{unbounded, Receiver, Sender};
use nats::{
    jetstream::{JetStream, SubscribeOptions},
    Message,
};
use tokio::sync::{broadcast, oneshot};

use crate::classroom::ClassroomId;

#[derive(Debug)]
enum Cmd {
    Subscribe {
        classroom_id: ClassroomId,
        resp_chan: oneshot::Sender<io::Result<broadcast::Receiver<Message>>>,
    },
    Shutdown,
}

pub struct NatsClient {
    tx: Sender<Cmd>,
    join_handle: thread::JoinHandle<()>,
}

impl NatsClient {
    pub fn new(nats_url: &str) -> anyhow::Result<Self> {
        let connection = nats::Options::with_credentials("nats.creds").connect(nats_url)?;

        let jetstream = nats::jetstream::new(connection);

        let (tx, rx) = unbounded();

        let join_handle = thread::spawn(move || nats_loop(jetstream, rx));
        Ok(Self { tx, join_handle })
    }

    pub async fn subscribe(
        &self,
        classroom_id: ClassroomId,
    ) -> Result<broadcast::Receiver<Message>, anyhow::Error> {
        let (resp_chan, resp_rx) = oneshot::channel();
        self.tx
            .try_send(Cmd::Subscribe {
                classroom_id,
                resp_chan,
            })
            .context("Failed to send cmd to nats loop")?;
        resp_rx
            .await
            .context("Failed to await receive from nats loop")?
            .context("Subscription failed")
    }

    pub fn shutdown(self) -> thread::JoinHandle<()> {
        let _ = self.tx.try_send(Cmd::Shutdown);

        self.join_handle
    }
}

fn nats_loop(js: JetStream, rx: Receiver<Cmd>) {
    let mut subscribers: HashMap<ClassroomId, Subscription> = HashMap::new();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Subscribe {
                classroom_id,
                resp_chan,
            } => {
                if let Some(sub) = subscribers.get(&classroom_id) {
                    resp_chan.send(Ok(sub.tx.subscribe()));
                } else {
                    resp_chan.send(add_new_subscription(&js, &mut subscribers, classroom_id));
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
