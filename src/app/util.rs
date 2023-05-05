use radix_trie::Trie;

#[derive(Debug, Clone)]
pub struct AudienceEstimator {
    inner: Trie<String, String>,
}

impl AudienceEstimator {
    pub fn new(config: &svc_authz::ConfigMap) -> Self {
        let mut inner = Trie::new();
        config.iter().for_each(|(key, _val)| {
            let rkey = key.split('.').rev().collect::<Vec<&str>>().join(".");
            inner.insert(rkey, key.clone());
        });
        Self { inner }
    }

    pub fn estimate(&self, audience: &str) -> Option<&str> {
        let rev_audience = audience.split('.').rev().collect::<Vec<&str>>().join(".");
        self.inner
            .get_ancestor_value(&rev_audience)
            .map(|aud| aud.as_str())
    }
}
