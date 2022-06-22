use serde_derive::Serialize;
use std::fmt::{Display, Formatter};
use svc_agent::AgentId;

#[derive(Serialize)]
pub struct Event {
    #[serde(rename = "type")]
    kind: String,
    label: Label,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Payload>,
}

#[derive(Serialize)]
pub enum Label {
    #[serde(rename = "agent.enter")]
    AgentEnter,
    #[serde(rename = "agent.leave")]
    AgentLeave,
}

impl Display for Label {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Label::AgentEnter => "agent.enter",
            Label::AgentLeave => "agent.leave",
        };

        write!(f, "{}", label)
    }
}

#[derive(Serialize)]
pub struct Payload {
    agent_id: AgentId,
}

impl Event {
    pub fn new(label: Label) -> Self {
        Self {
            kind: "event".to_string(),
            label,
            payload: None,
        }
    }

    pub fn payload(mut self, agent_id: AgentId) -> Self {
        self.payload = Some(Payload { agent_id });
        self
    }
}
