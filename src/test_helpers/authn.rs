use crate::test_helpers::*;
use svc_authn::jose::ConfigMap;

pub fn new() -> ConfigMap {
    let json = format!(
        r#"
        {{
            "{}": {{
                "algorithm": "ES256",
                "audience": ["{}"],
                "key": "{}"
            }}
        }}
        "#,
        TOKEN_ISSUER, USR_AUDIENCE, PUBKEY_PATH
    );

    serde_json::from_str(&json).expect("Failed to parse json string")
}
