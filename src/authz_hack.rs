/// Removes `.usr` and `.svc` from audience.
/// E.g. testing03.usr.example.org => testing03.example.org
pub fn remove_unwanted_paths_from_audience(audience: &str) -> String {
    audience.replace(".usr", "").replace(".svc", "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_unwanted_paths_from_audience_test() {
        assert_eq!(
            remove_unwanted_paths_from_audience("testing03.usr.example.org"),
            "testing03.example.org"
        );

        assert_eq!(
            remove_unwanted_paths_from_audience("testing03.svc.example.org"),
            "testing03.example.org"
        );
    }
}
