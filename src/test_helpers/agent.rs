use svc_agent::{mqtt::Address, AgentId};
use svc_authn::AccountId;

pub const API_VERSION: &str = "v1";

pub struct TestAgent {
    address: Address,
}

impl TestAgent {
    pub fn new(agent_label: &str, account_label: &str, audience: &str) -> Self {
        let account_id = AccountId::new(account_label, audience);
        let agent_id = AgentId::new(agent_label, account_id.clone());
        let address = Address::new(agent_id, API_VERSION);
        Self { address }
    }

    pub fn agent_id(&self) -> &AgentId {
        &self.address.id()
    }
}
