pub mod agent;
pub mod authz;
pub mod db;
pub mod factory;
pub mod state;
pub mod test_container;

pub mod prelude {
    pub use super::{
        agent::TestAgent, authz::TestAuthz, db::TestDb, factory, state::TestState,
        test_container::TestContainer, SVC_AUDIENCE, USR_AUDIENCE,
    };
}

pub const USR_AUDIENCE: &str = "dev.usr.example.com";
pub const SVC_AUDIENCE: &str = "dev.svc.example.org";
