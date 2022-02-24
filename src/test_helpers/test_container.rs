use testcontainers::{clients, images, Container, Docker};

pub struct PostgresHandle<'a> {
    pub connection_string: String,
    _container: Container<'a, clients::Cli, images::postgres::Postgres>,
}

pub struct TestContainer {
    docker: clients::Cli,
}

impl TestContainer {
    pub fn new() -> Self {
        Self {
            docker: clients::Cli::default(),
        }
    }

    pub fn run_postgres(&self) -> PostgresHandle {
        let image = images::postgres::Postgres::default();
        let node = self.docker.run(image);
        let connection_string = format!(
            "postgres://postgres:postgres@localhost:{}",
            node.get_host_port(5432).expect("get host port"),
        );
        PostgresHandle {
            connection_string,
            _container: node,
        }
    }
}
