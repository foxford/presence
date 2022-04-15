pub mod agent;
pub mod authn;
pub mod authz;
pub mod db;
pub mod factory;
pub mod state;
pub mod test_container;

pub mod prelude {
    pub use super::{
        agent::TestAgent, authn, authz::TestAuthz, db::TestDb, factory, state::TestState,
        test_container::TestContainer, PUBKEY_PATH, SVC_AUDIENCE, TOKEN_ISSUER, USR_AUDIENCE,
    };
}

pub const SVC_AUDIENCE: &str = "dev.example.org";
pub const USR_AUDIENCE: &str = "dev.example.com";
pub const TOKEN_ISSUER: &str = "iam.example.com";
pub const PUBKEY_PATH: &str = "data/keys/svc.public_key.p8.der.sample";
