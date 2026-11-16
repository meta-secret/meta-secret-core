use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::common::model::secret::{SsClaim, SsDistributionStatus};
use meta_secret_core::node::common::model::{ApplicationState, UserMemberFullInfo, VaultFullInfo};
use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use meta_secret_core::node::db::repo::generic_db::*;
use serde_json::Value;

async fn member(h: &Harness) -> Result<UserMemberFullInfo> {
    let ApplicationState::Vault(VaultFullInfo::Member(member)) =
        h.service().get_app_state().await?
    else {
        anyhow::bail!("real member state missing")
    };
    Ok(*member)
}
fn projection(member: &UserMemberFullInfo) -> Result<Value> {
    let serialized = serde_json::to_value(member)?;
    serialized.get("liveboardState").filter(|v| v.is_object()).cloned()
        .context("RED: real get_app_state member snapshot must carry the allowlisted Liveboard projection")
}
fn secret<'a>(state: &'a Value, pass: &str) -> Result<&'a Value> {
    state["secrets"]
        .as_array()
        .context("secrets array")?
        .iter()
        .find(|s| s["secretId"] == pass)
        .context("exact secret projection missing")
}

#[tokio::test]
async fn projection_real_one_two_three_member_storage_modes_and_capabilities() -> Result<()> {
    for (count, mode) in [(1, "whole"), (2, "copies"), (3, "shares")] {
        let h = Harness::new(count).await?;
        let state = projection(&member(&h).await?)?;
        assert_eq!(state["schemaVersion"], 1);
        assert_eq!(state["currentDeviceId"], h.approver.device_id().to_string());
        assert_eq!(state["storageMode"], mode);
        assert_eq!(state["canCreateSecret"], true);
        let ids = state["removableDeviceIds"]
            .as_array()
            .context("removable IDs")?;
        assert_eq!(ids.len(), count - 1);
        assert!(!ids.contains(&Value::String(h.approver.device_id().to_string())));
    }
    Ok(())
}

#[tokio::test]
async fn projection_show_local_requires_actual_usable_encrypted_share_not_member_count()
-> Result<()> {
    let mut h = Harness::new(2).await?;
    let pass = h.seed_secret("ready-local").await?;
    let state = projection(&member(&h).await?)?;
    assert_eq!(
        secret(&state, &pass.id_str())?["primaryAction"],
        "showLocal"
    );
    assert_eq!(secret(&state, &pass.id_str())?["readiness"], "ready");
    let data = h.distribution(&pass, &h.approver).await?;
    use meta_secret_core::node::common::model::secret::SsDistributionId;
    use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
    use meta_secret_core::node::db::events::generic_log_event::ObjIdExtractor;
    let obj =
        h.p.find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
            pass_id: pass.clone(),
            receiver: h.approver.device_id().clone(),
        }))
        .await?
        .context("seeded actual share")?;
    h.p.repo.try_delete(obj.obj_id()).await?;
    let state = projection(&member(&h).await?)?;
    assert_ne!(
        secret(&state, &pass.id_str())?["primaryAction"],
        "showLocal"
    );
    assert_ne!(secret(&state, &pass.id_str())?["readiness"], "ready");
    assert!(
        !serde_json::to_string(&state)?.contains(&serde_json::to_string(&data.secret_message)?)
    );
    Ok(())
}

#[tokio::test]
async fn projection_accepted_recovery_without_remote_material_never_advertises_show() -> Result<()>
{
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("accepted-without-packet").await?;
    let vm = h.vault.clone().to_vault_member(
        meta_secret_core::node::common::model::user::common::UserDataMember::from(
            h.approver.user(),
        ),
    )?;
    let mut claim: SsClaim = vm.create_recovery_claim(pass.clone());
    for (receiver, status) in claim.status.statuses.iter_mut() {
        *status = if receiver == h.initiator.device_id() {
            SsDistributionStatus::Sent
        } else {
            SsDistributionStatus::Declined
        };
    }
    PersistentSharedSecret::from(h.p.clone())
        .save_ss_log_event(claim)
        .await?;
    let state = projection(&member(&h).await?)?;
    let secret = secret(&state, &pass.id_str())?;
    assert_ne!(
        secret["primaryAction"], "showRecovered",
        "accepted status alone cannot make absent recovery packets usable"
    );
    assert_ne!(secret["readiness"], "ready");
    Ok(())
}

#[tokio::test]
async fn projection_json_allowlist_excludes_actual_encrypted_transfers_and_raw_logs() -> Result<()>
{
    let mut h = Harness::new(3).await?;
    h.seed_secret("metadata-only").await?;
    let request = h.seed_removal().await?;
    let current = member(&h).await?;
    let state = projection(&current)?;
    let allowed = [
        "schemaVersion",
        "vaultName",
        "currentDeviceId",
        "storageMode",
        "canCreateSecret",
        "removableDeviceIds",
        "secrets",
        "operations",
    ];
    assert!(
        state
            .as_object()
            .context("object")?
            .keys()
            .all(|k| allowed.contains(&k.as_str()))
    );
    let secret_fields = [
        "secretId",
        "primaryAction",
        "recoveryClaimId",
        "deletionRequestId",
        "canDelete",
        "readiness",
    ];
    for entry in state["secrets"].as_array().context("secrets")? {
        assert!(
            entry
                .as_object()
                .context("secret metadata")?
                .keys()
                .all(|k| secret_fields.contains(&k.as_str()))
        );
    }
    let operation_fields = [
        "kind",
        "requestId",
        "requesterId",
        "targetDeviceId",
        "secretId",
        "clientStatus",
        "eligiblePeerIds",
        "canApprove",
        "canDecline",
        "outcome",
        "responderId",
        "resolutionSource",
        "readiness",
    ];
    for entry in state["operations"].as_array().context("operations")? {
        assert!(
            entry
                .as_object()
                .context("operation metadata")?
                .keys()
                .all(|k| operation_fields.contains(&k.as_str()))
        );
    }
    let json = serde_json::to_string(&state)?;
    assert!(!json.contains(VALUE));
    assert!(!json.contains(&request.share_transfers[0].encrypted_share));
    for forbidden in [
        "shareTransfers",
        "encryptedShare",
        "secretMessage",
        "ssClaims",
        "vaultEvents",
        "source",
        "signature",
        "signedActionHash",
        "liveboardRemovalApplications",
    ] {
        assert!(!json.contains(forbidden), "projection leaked {forbidden}");
    }
    // Native member JSON must preserve the same public projection, including old JSON compatibility.
    let mut old = serde_json::to_value(&current)?;
    old.as_object_mut().unwrap().remove("liveboardState");
    let _: UserMemberFullInfo = serde_json::from_value(old)?;
    // The actual WASM getter receives a separate compile-ready assertion when exported.
    Ok(())
}
