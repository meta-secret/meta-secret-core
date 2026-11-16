use anyhow::Result;
use meta_secret_core::node::api::CanonicalDecisionInput;

pub const ID: &str = "d732b6ac-0881-42ac-abd6-a5cba32b34ec";
pub fn valid() -> String {
    format!(r#"{{"requestId":"{ID}","accepted":true}}"#)
}
pub fn invalid() -> Vec<String> {
    vec![
        "private-input-marker".into(),
        r#"{"requestId":"","accepted":false}"#.into(),
        r#"{"requestId":"private-input-marker","accepted":true}"#.into(),
        format!(r#"{{"requestId":"{ID}"}}"#),
        format!(r#"{{"requestId":"{ID}","accepted":"yes"}}"#),
        format!(
            r#"{{"requestId":"{ID}","accepted":true,"targetDeviceId":"private-input-marker"}}"#
        ),
        format!(
            r#"{{"requestId":"{ID}","accepted":true,"candidate":{{"private-input-marker":true}}}}"#
        ),
        format!(r#"{{"requestId":"{ID}","accepted":true,"accepted":false}}"#),
        format!("{}{}", valid(), " ".repeat(1024)),
        format!(
            r#"{{"requestId":"{ID}","accepted":true,"{}":0}}"#,
            "я".repeat(512)
        ),
    ]
}

#[test]
fn shared_core_decision_parser_accepts_only_exact_valid_uuid_and_boolean() -> Result<()> {
    let expected = CanonicalDecisionInput {
        request_id: ID.into(),
        accepted: true,
    };
    assert_eq!(CanonicalDecisionInput::parse_json(&valid())?, expected);
    let boundary = format!("{}{}", valid(), " ".repeat(1024 - valid().len()));
    assert_eq!(boundary.len(), 1024);
    assert_eq!(CanonicalDecisionInput::parse_json(&boundary)?, expected);
    Ok(())
}

#[test]
fn shared_core_decision_parser_rejects_ambiguous_unbounded_or_target_fallback_input_safely() {
    for input in invalid() {
        let error = CanonicalDecisionInput::parse_json(&input)
            .expect_err("invalid input must fail before a lookup/write");
        assert_eq!(error.to_string(), "InvalidDecisionInput");
        assert!(!error.to_string().contains("private-input-marker"));
    }
}
