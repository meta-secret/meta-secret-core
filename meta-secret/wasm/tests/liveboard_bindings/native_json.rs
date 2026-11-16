use super::decisions;
use serde_json::Value;

type JsonDecision = fn(String) -> String;

fn response(json: String) -> Value {
    serde_json::from_str(&json).expect("native facade must return valid JSON")
}

#[test]
fn native_and_uniffi_exact_decisions_validate_before_global_manager_lookup() {
    let pairs: [(JsonDecision, JsonDecision); 2] = [
        (
            mobile_common::json_api::decide_join_request,
            metasecret_mobile::decide_join_request,
        ),
        (
            mobile_common::json_api::decide_remove_member,
            metasecret_mobile::decide_remove_member,
        ),
    ];
    for (common, uniffi) in pairs {
        for input in decisions::invalid() {
            let common_result = response(common(input.clone()));
            assert_eq!(common_result, response(uniffi(input)));
            assert_eq!(common_result["success"], false);
            assert_eq!(common_result["errorCode"], "InvalidDecisionInput");
            assert!(common_result.get("message").is_none());
            assert!(!common_result.to_string().contains("private-input-marker"));
        }
    }
}

#[test]
fn native_and_uniffi_checked_calls_fail_closed_without_initialized_user_database() {
    let pass = meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str(
        "private-pass",
    )
    .id_str();
    let calls = [
        (
            mobile_common::json_api::cluster_distribution_checked(
                "private-name".into(),
                "private-secret".into(),
            ),
            metasecret_mobile::cluster_distribution_checked(
                "private-name".into(),
                "private-secret".into(),
            ),
        ),
        (
            mobile_common::json_api::recover_checked(pass.clone()),
            metasecret_mobile::recover_checked(pass),
        ),
        (
            mobile_common::json_api::decide_join_request(decisions::valid()),
            metasecret_mobile::decide_join_request(decisions::valid()),
        ),
        (
            mobile_common::json_api::decide_remove_member(decisions::valid()),
            metasecret_mobile::decide_remove_member(decisions::valid()),
        ),
    ];
    for (common, uniffi) in calls {
        let actual = response(common);
        assert_eq!(actual, response(uniffi));
        assert_eq!(actual["success"], false);
        assert_eq!(actual["errorCode"], "NotInitialized");
        assert!(actual.get("message").is_none());
        assert!(!actual.to_string().contains("private-"));
    }
    assert_eq!(
        response(mobile_common::json_api::get_state()),
        response(metasecret_mobile::get_state())
    );
}

#[test]
fn uniffi_udl_additive_contract_preserves_legacy_exports() {
    let udl = include_str!("../../../mobile/uniffi/src/mobile_uniffi.udl");
    for declaration in [
        "string cluster_distribution_checked(string name, string secret);",
        "string recover_checked(string secret_id);",
        "string decide_join_request(string request_json);",
        "string decide_remove_member(string request_json);",
        "string split_secret(string secret_id, string secret);",
        "string recover(string secret_id);",
        "string update_membership(string candidate, string action_update);",
        "string approve_remove_member(string target_device_id);",
        "string decline_remove_member(string target_device_id);",
    ] {
        assert!(
            udl.contains(declaration),
            "missing additive or legacy UDL contract: {declaration}"
        );
    }
}
