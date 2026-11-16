use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::{
    ApplicationState, VaultFullInfo,
    crypto::aead::EncryptedMessage,
    secret::{SecretDistributionData, SsDistributionStatus},
    user::common::UserDataMember,
};
use meta_secret_core::node::db::{
    descriptors::{
        shared_secret_descriptor::SsWorkflowDescriptor, vault_descriptor::DeviceLogDescriptor,
    },
    events::{
        kv_log_event::*,
        shared_secret_event::SsWorkflowObject,
        vault::{
            device_log_event::DeviceLogObject,
            vault_log_event::{VaultActionEvent, VaultActionRequestEvent},
        },
    },
    objects::persistent_shared_secret::PersistentSharedSecret,
    repo::generic_db::*,
};
use meta_secret_core::secret::shared_secret::PlainText;
use serde_json::Value;

async fn state(h: &Harness) -> Result<Value> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("actual member state missing")
    };
    Ok(serde_json::to_value(member)?["liveboardState"].clone())
}

#[tokio::test]
async fn actual_material_projection_does_not_reveal_consume_or_complete_recovery() -> Result<()> {
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("projection-is-observation").await?;
    let vm = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?;
    let mut claim = vm.create_recovery_claim(pass.clone());
    claim
        .status
        .statuses
        .insert(h.initiator.device_id().clone(), SsDistributionStatus::Sent);
    claim
        .status
        .statuses
        .insert(h.target.device_id().clone(), SsDistributionStatus::Declined);
    let share = h.share(&pass, &h.initiator).await?;
    let packet = h
        .initiator
        .device_creds
        .key_manager()?
        .transport
        .encrypt_string(
            PlainText::from(share.as_json()?),
            &h.approver.device().keys.transport_pk,
        )?;
    let recovery = claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == *h.initiator.device_id())
        .context("actual receiver")?;
    h.p.repo
        .save(SsWorkflowObject::Recovery(KvLogEvent {
            key: KvKey::from(SsWorkflowDescriptor::Recovery(recovery)),
            value: SecretDistributionData {
                vault_name: h.vault.vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: EncryptedMessage::CipherShare { share: packet },
            },
        }))
        .await?;
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    let before = h.p.repo.get_db().await;
    let projection = state(&h).await?;
    assert!(
        projection["secrets"]
            .as_array()
            .context("secrets")?
            .iter()
            .any(|s| s["secretId"] == pass.id_str() && s["primaryAction"] == "showRecovered")
    );
    assert!(!serde_json::to_string(&projection)?.contains(VALUE));
    assert!(
        h.p.repo.get_db().await == before,
        "reading ready material may not consume ciphertext or mark Delivered"
    );
    Ok(())
}

#[tokio::test]
async fn recreated_secret_does_not_inherit_tombstoned_recovery_or_old_pass_identity() -> Result<()>
{
    let mut h = Harness::new(3).await?;
    let old = h.seed_secret("same-display-name").await?;
    let vm = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?;
    let claim = vm.create_recovery_claim(old.clone());
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim.clone())
        .await?;
    h.vault = h.vault.clone().delete_secret(old.clone());
    // Distinct actual new ID with the same name, as selected by the real distributor.
    h.save_vault().await?;
    let receipt = h
        .service()
        .cluster_distribution_checked(
            meta_secret_core::node::common::model::meta_pass::PlainPassInfo {
                pass_id: old.clone(),
                pass: VALUE.into(),
            },
        )
        .await?;
    let actual_id = receipt.secret_id.context("actual recreated ID")?;
    assert_ne!(actual_id, old.id_str());
    let actual_events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    let new = actual_events
        .into_iter()
        .find_map(|event| match event.0.value {
            VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(add))
                if add.meta_pass_id.id_str() == actual_id =>
            {
                Some(add.meta_pass_id)
            }
            _ => None,
        })
        .context("the actual persisted recreated Pass ID")?;
    h.vault = h.vault.clone().add_secret(new.clone());
    h.save_vault().await?;
    let projection = state(&h).await?;
    assert_eq!(
        projection["secrets"].as_array().context("secrets")?.len(),
        1
    );
    assert_eq!(projection["secrets"][0]["secretId"], new.id_str());
    assert!(
        projection["secrets"][0]
            .get("recoveryClaimId")
            .is_none_or(Value::is_null)
    );
    assert!(
        !projection["operations"]
            .as_array()
            .context("ops")?
            .iter()
            .any(|op| op["requestId"] == claim.id.wasm_id_str())
    );
    Ok(())
}

#[tokio::test]
async fn stale_member_status_cannot_advertise_secret_or_removal_actions_after_actual_membership_removed()
-> Result<()> {
    let mut h = Harness::new(3).await?;
    h.seed_secret("actual-membership-required").await?;
    h.vault.users.remove(h.approver.device_id());
    h.save_vault().await?;
    // The persisted status is intentionally stale, as can occur across separate
    // canonical records. Projection must fail closed against actual membership.
    let result = state(&h).await;
    if let Ok(projection) = result {
        assert_eq!(projection["canCreateSecret"], false);
        assert!(
            projection["removableDeviceIds"]
                .as_array()
                .context("removable")?
                .is_empty(),
            "nonmember projection must not advertise removable peers"
        );
        assert!(
            projection["secrets"]
                .as_array()
                .context("secrets")?
                .iter()
                .all(|s| s["canDelete"] == false && s["primaryAction"] == "unavailable")
        );
    }
    Ok(())
}
