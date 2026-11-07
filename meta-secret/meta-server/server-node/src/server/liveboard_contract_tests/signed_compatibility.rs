use super::*;
use meta_secret_core::crypto::keys::DsaPk;
use meta_secret_core::node::common::model::device::common::DeviceId;
use serde::{Deserialize, Serialize};

// Frozen pre-Liveboard typed decoder: the old payload variants and removal
// request/update shape intentionally do not reuse the current signed structs.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldSignedAction {
    signer: DeviceId,
    nonce: u64,
    stream: String,
    signature: Base64Text,
    payload: OldPayload,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OldPayload {
    Event(Box<OldEvent>),
    RecoveryCompletion(meta_secret_core::node::api::SsRecoveryCompletion),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OldEvent {
    DeviceLog(KvLogEvent<OldAction>),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OldAction {
    Request(Box<OldRequest>),
    Update(Box<OldUpdate>),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OldRequest {
    RemoveMember(OldRemovalRequest),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum OldUpdate {
    RemoveMember(OldRemovalUpdate),
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldRemovalRequest {
    sender: UserDataMember,
    target: UserDataMember,
    #[serde(default)]
    share_transfers: Vec<OldTransfer>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldTransfer {
    pass_id: meta_secret_core::node::common::model::meta_pass::MetaPasswordId,
    receiver: DeviceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claim_id: Option<meta_secret_core::node::common::model::secret::ClaimId>,
    encrypted_share: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OldRemovalUpdate {
    request: OldRemovalRequest,
    sender: UserDataMember,
    target: UserDataMember,
    new_status: UserMembership,
    #[serde(default)]
    decision: RemoveMemberDecision,
    #[serde(default)]
    share_transfers: Vec<OldTransfer>,
}

impl OldSignedAction {
    fn verify(&self, key: &DsaPk) -> Result<()> {
        #[derive(Serialize)]
        struct Unsigned<'a> {
            signer: &'a DeviceId,
            nonce: u64,
            stream: &'a str,
            payload: &'a OldPayload,
        }
        let bytes = canonical_json(&Unsigned {
            signer: &self.signer,
            nonce: self.nonce,
            stream: &self.stream,
            payload: &self.payload,
        })?;
        key.verify(&String::from_utf8(bytes)?, &self.signature)
    }
}

#[tokio::test]
async fn liveboard_old_typed_request_and_decision_signatures_stay_byte_compatible() -> Result<()> {
    let h = Harness::new().await?;
    let event = next_event(
        &h.p_obj,
        &h.a,
        VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(h.removal())),
    )
    .await?;
    let current = sign_event(
        event.to_generic(),
        h.a.device_id().clone(),
        &h.a.device_creds.key_manager()?.dsa,
    )?;
    let old: OldSignedAction = serde_json::from_value(serde_json::to_value(&current)?)?;
    old.verify(&h.a.device().keys.dsa_pk)?;
    let old_wire = serde_json::to_value(old)?;
    let decoded: SignedAction = serde_json::from_value(old_wire.clone())?;
    decoded.verify(&h.a.device().keys.dsa_pk)?;
    assert!(
        canonical_json(&old_wire)? == canonical_json(&decoded)?,
        "the exact signed request representation must survive old/new decoding"
    );
    send(&h.server, decoded).await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let current = sign_event(
        event.to_generic(),
        h.c.device_id().clone(),
        &h.c.device_creds.key_manager()?.dsa,
    )?;
    let old: OldSignedAction = serde_json::from_value(serde_json::to_value(&current)?)?;
    old.verify(&h.c.device().keys.dsa_pk)?;
    let decoded: SignedAction = serde_json::from_value(serde_json::to_value(old)?)?;
    decoded.verify(&h.c.device().keys.dsa_pk)?;
    send(&h.server, decoded).await?;
    let vault = PersistentVault::from(h.p_obj)
        .get_vault(h.a.vault_name)
        .await?;
    assert_eq!(
        vault.0.value.members().len(),
        3,
        "legacy decline must preserve membership"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_guarded_signature_binds_ref_version_and_entire_body_and_old_decoder_rejects()
-> Result<()> {
    let h = Harness::new().await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let wire = signed_wire(
        &h.c,
        &event,
        json!({"guardedRemovalDecision":{"version":1,
        "requestId":"15f0b92a-e4bf-45c7-93ee-7ef59594cbd4", "event":event}}),
    )?;
    assert!(
        serde_json::from_value::<OldSignedAction>(wire.clone()).is_err(),
        "old typed decoder must reject the distinct variant before application"
    );
    let current: SignedAction = serde_json::from_value(wire.clone())
        .context("RED: new Core cannot decode the additive guarded signed variant")?;
    current.verify(&h.c.device().keys.dsa_pk)?;
    assert!(
        current.event().is_err(),
        "legacy event coercion must not silently discard the exact guard"
    );
    assert!(current.verify(&h.a.device().keys.dsa_pk).is_err());
    for (field, changed) in [
        ("requestId", json!("e426d6ce-f987-4b89-bf0c-ae226533b555")),
        ("version", json!(2)),
    ] {
        let mut tampered = wire.clone();
        tampered["payload"]["guardedRemovalDecision"][field] = changed;
        let decoded: SignedAction = serde_json::from_value(tampered)?;
        assert!(
            decoded.verify(&h.c.device().keys.dsa_pk).is_err(),
            "the signed guard field cannot be tampered"
        );
    }
    let mut tampered = wire;
    tampered["payload"]["guardedRemovalDecision"]["event"]["value"]["update"]["removeMember"]["target"] =
        serde_json::to_value(member(&h.a))?;
    assert!(
        serde_json::from_value::<SignedAction>(tampered)?
            .verify(&h.c.device().keys.dsa_pk)
            .is_err(),
        "the complete nested removal body must be signed"
    );
    Ok(())
}

#[tokio::test]
async fn liveboard_wrong_version_guarded_signature_is_rejected_without_effect_even_when_crypto_valid()
-> Result<()> {
    let h = Harness::new().await?;
    h.request().await?;
    let request_id = h.pending_id().await?;
    let event = next_event(
        &h.p_obj,
        &h.c,
        VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(h.decision(false))),
    )
    .await?;
    let action = guarded(&h.c, &event, &request_id, 99)?;
    action.verify(&h.c.device().keys.dsa_pk)?;
    let before = h.p_obj.repo.get_db().await;
    assert!(
        send(&h.server, action).await.is_err(),
        "valid crypto alone does not authorize an unsupported protocol version"
    );
    assert!(h.p_obj.repo.get_db().await == before);
    Ok(())
}
