//! Typed identity wire shared with the public gateway; no credentials in list rows.
pub use mg_live::gateway::AiSummary;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentReply {
    pub ids: Vec<String>,
}
pub fn decode_list(value: serde_json::Value) -> Result<Vec<AiSummary>, serde_json::Error> {
    serde_json::from_value(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn normative_identity_and_consent_responses_decode_without_legacy_fields() {
        let wire = json!([{"id":"ai:one","name":"One","owner":"dm:owner","avatar":"https://example.invalid/a.png","llm":false}]);
        let rows = decode_list(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&rows).unwrap(), wire);
        assert!(decode_list(json!({"items":wire})).is_err());
        assert_eq!(
            serde_json::from_value::<ConsentReply>(json!({"ids":["ai:one"]})).unwrap().ids,
            vec!["ai:one"]
        );
    }
}
