pub mod agent;
pub mod db;
pub mod factory;
pub mod state;
pub mod test_container;

pub mod prelude {
    pub use super::{
        agent::TestAgent, db::TestDb, factory, state::TestState, test_container::TestContainer,
        USR_AUDIENCE,
    };
}

pub const USR_AUDIENCE: &'static str = "dev.usr.example.com";
