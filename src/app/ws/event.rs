use serde_derive::Serialize;
use svc_agent::AgentId;

#[derive(Serialize)]
pub struct Event {
    #[serde(rename = "type")]
    kind: String,
    pub label: Label,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Payload>,
}

#[derive(Serialize)]
pub enum Label {
    #[serde(rename = "agent.enter")]
    AgentEnter,
    #[serde(rename = "agent.leave")]
    AgentLeave,
    #[serde(rename = "agent.replaced")]
    AgentReplaced,
    #[serde(rename = "agent.auth_timed_out")]
    AuthTimedOut,
    #[serde(rename = "agent.pong_timed_out")]
    PongTimedOut,
}

impl ToString for Label {
    fn to_string(&self) -> String {
        let label = match self {
            Label::AgentEnter => "agent.enter",
            Label::AgentLeave => "agent.leave",
            Label::AgentReplaced => "agent.replaced",
            Label::AuthTimedOut => "agent.auth_timed_out",
            Label::PongTimedOut => "agent.pong_timed_out",
        };

        String::from(label)
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
