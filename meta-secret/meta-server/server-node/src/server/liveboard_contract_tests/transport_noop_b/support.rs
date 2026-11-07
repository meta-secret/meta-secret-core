use super::*;
use meta_secret_core::crypto::utils::sha256_hex;

pub(super) fn signed_value(actor: &UserCreds, id: ArtifactId) -> Result<Value> {
    let value = json!({"version":1,"purpose":"meta-secret.device-transport-noop.v1","vaultName":actor.vault_name,"actorId":actor.device_id(),"eventId":id,"stream":id.fqdn.clone().id_str(),"nonce":id.id.curr as u64,"clientEntryCommitment":sha256_hex("B opaque audit correlation; no business historical proof")});
    sign_value(actor, value)
}
pub(super) fn sign_value(actor: &UserCreds, mut value: Value) -> Result<Value> {
    value
        .as_object_mut()
        .context("authorization object")?
        .remove("signature");
    let bytes = String::from_utf8(canonical_json(&value)?)?;
    value["signature"] = serde_json::to_value(actor.device_creds.key_manager()?.dsa.sign(bytes))?;
    Ok(value)
}
pub(super) fn decode(value: &Value) -> Result<SyncRequest> {
    Ok(serde_json::from_str(&serde_json::to_string(
        &json!({"write":{"deviceTransportNoop":value}}),
    )?)?)
}
pub(super) async fn submit<R: KvLogEventRepo>(
    server: &ServerApp<R>,
    value: &Value,
) -> Result<Value> {
    let response = server.handle_client_request(decode(value)?).await?;
    let DataSyncResponse::DeviceTransportNoop(receipt) = response else {
        anyhow::bail!("real dedicated Noop receipt required")
    };
    let result = serde_json::to_value(receipt)?;
    assert_eq!(result["eventId"], value["eventId"]);
    assert_eq!(
        result["clientEntryCommitment"],
        value["clientEntryCommitment"]
    );
    assert_eq!(
        result["authorizationHash"],
        sha256_hex(&String::from_utf8(canonical_json(value)?)?)
    );
    assert_eq!(result["recordEffect"], "not_applied");
    assert_eq!(result["transportState"], "noop_committed");
    assert!(!serde_json::to_string(&result)?.contains("signature"));
    Ok(result)
}
pub(super) async fn next<R: KvLogEventRepo>(h: &Seed<R>) -> Result<ArtifactId> {
    use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
    Ok(PersistentDeviceLog::from(h.p.clone())
        .find_tail_id(&h.actor.user_id())
        .await?
        .map(|id| id.next())
        .unwrap_or_else(|| ArtifactId::from(DeviceLogDescriptor::from(h.actor.user_id()))))
}
/// Initial reader DB fixture only: exact real Core pending Split records and
/// encrypted packets. This does not claim signed Create publication/lifecycle.
pub(super) async fn project_initial_split_records<R: KvLogEventRepo>(h: &Seed<R>) -> Result<()> {
    use meta_secret_core::node::db::{
        descriptors::shared_secret_descriptor::SsDeviceLogDescriptor,
        objects::persistent_shared_secret::PersistentSharedSecret,
    };
    let records = h
        .p
        .get_object_events_from_beginning(SsDeviceLogDescriptor::from(h.actor.device_id().clone()))
        .await?;
    assert!(
        !records.is_empty(),
        "real Core-created pending Split source required"
    );
    let p_ss = PersistentSharedSecret::from(h.p.clone());
    for record in records {
        p_ss.save_ss_log_event(record.0.value).await?;
    }
    Ok(())
}
pub(super) async fn normal<R: KvLogEventRepo>(
    h: &Seed<R>,
    server: &ServerApp<R>,
) -> Result<DeviceLogObject> {
    let source = next_event(&h.p, &h.actor, VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(
        meta_secret_core::node::db::events::vault::vault_log_event::AddMetaPassEvent {
            sender: member(&h.actor),
            meta_pass_id: meta_secret_core::node::common::model::meta_pass::MetaPasswordId::build_from_str("B actual next business command"),
        },
    ))).await?;
    send(
        server,
        sign_event(
            source.clone().to_generic(),
            h.actor.device_id().clone(),
            &h.actor.device_creds.key_manager()?.dsa,
        )?,
    )
    .await?;
    Ok(source)
}
