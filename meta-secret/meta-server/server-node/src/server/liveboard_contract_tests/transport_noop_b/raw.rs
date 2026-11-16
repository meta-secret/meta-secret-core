use super::{support::*, *};

async fn malformed(level: usize, duplicate: bool) -> Result<()> {
    let actor = FixtureRegistry::empty().state.user_creds.client;
    let id = ArtifactId::from(DeviceLogDescriptor::from(actor.user_id()));
    let signed = signed_value(&actor, id)?;
    let raw = serde_json::to_string(&json!({"write":{"deviceTransportNoop":signed}}))?;
    assert!(serde_json::from_str::<SyncRequest>(&raw).is_ok());
    let (object, field) = match level {
        0 => (&signed["eventId"], "fqdn"),
        1 => (&signed["eventId"]["fqdn"], "objType"),
        _ => (&signed["eventId"]["id"], "curr"),
    };
    let original = serde_json::to_string(object)?;
    let inserted = if duplicate {
        format!(
            "{{\"{field}\":{},{}",
            serde_json::to_string(&object[field])?,
            &original[1..]
        )
    } else {
        format!("{{\"unexpectedNestedField\":0,{}", &original[1..])
    };
    let changed = raw.replacen(&original, &inserted, 1);
    assert_ne!(raw, changed);
    // RAW input is essential: Value would erase duplicate object fields.
    assert!(
        serde_json::from_str::<SyncRequest>(&changed).is_err(),
        "strict Noop nested field boundary accepted raw extra/duplicate"
    );
    Ok(())
}
macro_rules! raw_case {
    ($name:ident,$level:expr,$dup:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            malformed($level, $dup).await
        }
    };
}
raw_case!(noop_b_raw_artifact_id_extra, 0, false);
raw_case!(noop_b_raw_fqdn_extra, 1, false);
raw_case!(noop_b_raw_sequence_extra, 2, false);
raw_case!(noop_b_raw_artifact_id_same_value_duplicate, 0, true);
raw_case!(noop_b_raw_fqdn_same_value_duplicate, 1, true);
raw_case!(noop_b_raw_sequence_same_value_duplicate, 2, true);

#[tokio::test]
async fn noop_b_raw_complete_max_sequence_cannot_overflow_next() -> Result<()> {
    let actor = FixtureRegistry::empty().state.user_creds.client;
    let mut auth = signed_value(
        &actor,
        ArtifactId::from(DeviceLogDescriptor::from(actor.user_id())),
    )?;
    auth["eventId"]["id"] = json!({"curr":usize::MAX,"prev":usize::MAX-1});
    auth["nonce"] = json!(u64::MAX);
    let signed = sign_value(&actor, auth)?;
    assert!(decode(&signed).is_err());
    Ok(())
}
#[tokio::test]
async fn noop_b_raw_numeric_sequence_overflow_rejected_before_lossy_value() -> Result<()> {
    let actor = FixtureRegistry::empty().state.user_creds.client;
    let auth = signed_value(
        &actor,
        ArtifactId::from(DeviceLogDescriptor::from(actor.user_id())),
    )?;
    let raw = serde_json::to_string(&json!({"write":{"deviceTransportNoop":auth}}))?;
    let overflow = (u128::from(u64::MAX) + 1).to_string();
    let changed = raw.replacen("\"curr\":1", &format!("\"curr\":{overflow}"), 1);
    assert_ne!(raw, changed);
    assert!(serde_json::from_str::<SyncRequest>(&changed).is_err());
    Ok(())
}
