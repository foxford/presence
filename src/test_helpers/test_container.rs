use testcontainers::{clients, images, Container};

pub struct PostgresHandle<'a> {
    pub connection_string: String,
    _container: Container<'a, images::postgres::Postgres>,
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
            node.get_host_port_ipv4(5432)
        );
        PostgresHandle {
            connection_string,
            _container: node,
        }
    }
}
