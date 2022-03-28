use crate::test_helpers::TOKEN_ISSUER;
use once_cell::sync::Lazy;
use svc_agent::{mqtt::Address, AgentId};
use svc_authn::{jose::Algorithm, token::jws_compact::TokenBuilder, AccountId, Authenticable};

pub const API_VERSION: &str = "v1";
const TOKEN_EXPIRATION: i64 = 600;
const KEY_PATH: &str = "data/keys/svc.private_key.p8.der.sample";

static PRIVATE_KEY: Lazy<Vec<u8>> =
    Lazy::new(|| std::fs::read(KEY_PATH).expect("Failed to read private key file"));

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

    pub fn account_id(&self) -> &AccountId {
        &self.address.id().as_account_id()
    }

    pub fn token(&self) -> String {
        TokenBuilder::new()
            .issuer(TOKEN_ISSUER)
            .subject(self.account_id())
            .expires_in(TOKEN_EXPIRATION)
            .key(Algorithm::ES256, PRIVATE_KEY.as_slice())
            .build()
            .expect("Failed to build access token")
    }
}
