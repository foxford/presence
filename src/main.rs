#[cfg(test)]
mod test_helpers;

fn main() {}

#[cfg(test)]
mod test {
    use crate::test_helpers::test_container::TestContainer;

    #[test]
    fn test_containers_works() {
        let test_container = TestContainer::new();
        let postgres = test_container.run_postgres();

        dbg!(postgres.connection_string);
        assert!(true);
    }
}
