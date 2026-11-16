//! Real persisted input/source controls; no UI or copied matching proof fixtures.
#[path = "liveboard_reveal/boundaries.rs"]
mod boundaries;
#[path = "liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "liveboard_reveal/fixture.rs"]
mod fixture;
#[path = "liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "liveboard_reveal/legacy.rs"]
mod legacy;
#[path = "liveboard_reveal/sources.rs"]
mod sources;
#[path = "guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result, bail, ensure};
use meta_secret_core::node::db::{
    events::{generic_log_event::ObjIdExtractor, vault::vault_event::VaultObject},
    objects::persistent_vault::PersistentVault,
    repo::generic_db::SaveCommand,
};
use serde_json::Value;

fn witnessed_secret_orders(original: &VaultObject) -> Result<[VaultObject; 2]> {
    ensure!(
        original.0.value.secrets.len() == 2,
        "TWO_SECRETS_SETUP_HOLD"
    );
    let bytes =
        serde_json::to_vec(original).map_err(|_| anyhow::anyhow!("VAULT_SERIALIZE_SETUP_HOLD"))?;
    let view: Value =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("VAULT_VIEW_SETUP_HOLD"))?;
    let order = &view["value"]["secrets"];
    ensure!(
        order.as_array().is_some_and(|items| items.len() == 2),
        "SET_ORDER_SHAPE_SETUP_HOLD"
    );
    for _ in 0..128 {
        let candidate: VaultObject = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("VAULT_REHYDRATE_SETUP_HOLD"))?;
        ensure!(
            candidate == *original && candidate.obj_id() == original.obj_id(),
            "TYPED_SOURCE_SETUP_HOLD"
        );
        let candidate_view = serde_json::to_value(&candidate)
            .map_err(|_| anyhow::anyhow!("CANDIDATE_VIEW_SETUP_HOLD"))?;
        if candidate_view["value"]["secrets"] != *order {
            return Ok([original.clone(), candidate]);
        }
    }
    bail!("SET_ORDER_WITNESS_SETUP_HOLD")
}

#[tokio::test]
async fn equal_typed_vault_with_two_secret_orders_has_one_reveal_vault_source() -> Result<()> {
    let mut r = fixture::Recovery::new()
        .await
        .map_err(|_| anyhow::anyhow!("RECOVERY_SETUP_HOLD"))?;
    let other =
        r.h.seed_secret("invariance-other-controlled")
            .await
            .map_err(|_| anyhow::anyhow!("SECOND_SECRET_SETUP_HOLD"))?;
    ensure!(other != r.pass, "DISTINCT_SECRET_SETUP_HOLD");
    let vault = PersistentVault::from(r.h.p.clone())
        .get_vault(r.h.vault.vault_name.clone())
        .await
        .map_err(|_| anyhow::anyhow!("VAULT_SOURCE_SETUP_HOLD"))?;
    let representations = witnessed_secret_orders(&vault)?;
    let expected_id = serde_json::to_value(vault.obj_id())
        .map_err(|_| anyhow::anyhow!("SOURCE_ID_SETUP_HOLD"))?;
    ensure!(expected_id.is_object(), "SOURCE_ID_SHAPE_SETUP_HOLD");
    let mut contexts = Vec::new();
    for candidate in representations {
        let order = serde_json::to_value(&candidate)
            .map_err(|_| anyhow::anyhow!("ORDER_VIEW_SETUP_HOLD"))?["value"]["secrets"]
            .clone();
        r.h.p
            .repo
            .replace(candidate.clone())
            .await
            .map_err(|_| anyhow::anyhow!("SOURCE_REPLACE_SETUP_HOLD"))?;
        let state = r
            .state()
            .await
            .map_err(|_| anyhow::anyhow!("ACCEPTED_STATE_SETUP_HOLD"))?;
        let accepted = state["operations"]
            .as_array()
            .and_then(|ops| {
                ops.iter()
                    .find(|op| op["requestId"] == r.claim.id.wasm_id_str())
            })
            .context("SELECTED_OPERATION_SETUP_HOLD")?;
        ensure!(
            accepted["clientStatus"] == "accepted",
            "ACCEPTED_STATUS_SETUP_HOLD"
        );
        let mut context = None;
        // Same exact source/order immediately before and after the actual consumer.
        for before_context in [true, false] {
            let actual = PersistentVault::from(r.h.p.clone())
                .get_vault(r.h.vault.vault_name.clone())
                .await
                .map_err(|_| anyhow::anyhow!("SOURCE_READBACK_SETUP_HOLD"))?;
            ensure!(
                actual == candidate && actual.obj_id() == vault.obj_id(),
                "SOURCE_READBACK_SETUP_HOLD"
            );
            let actual_view = serde_json::to_value(&actual)
                .map_err(|_| anyhow::anyhow!("READBACK_VIEW_SETUP_HOLD"))?;
            ensure!(
                actual_view["value"]["secrets"] == order,
                "ORDER_READBACK_SETUP_HOLD"
            );
            if before_context {
                context = r
                    .context()
                    .await
                    .map_err(|_| anyhow::anyhow!("CONTEXT_SETUP_HOLD"))?;
            }
        }
        let context = context
            .filter(Value::is_object)
            .context("ACCEPTED_PROOF_SETUP_HOLD")?;
        ensure!(context["version"] == 1, "CONTEXT_VERSION_SETUP_HOLD");
        ensure!(
            context["vaultSource"]["eventId"].is_object()
                && context["vaultSource"]["eventId"] == expected_id,
            "VAULT_EVENT_ID_SETUP_HOLD"
        );
        contexts.push(context);
    }
    for field in [
        "version",
        "vaultName",
        "currentDeviceId",
        "localDistributionSource",
        "recoverySources",
        "claimScopeCommitment",
    ] {
        ensure!(
            !contexts[0][field].is_null() && contexts[0][field] == contexts[1][field],
            "UNRELATED_CONTEXT_SETUP_HOLD"
        );
    }
    ensure!(
        contexts[0]["vaultSource"] == contexts[1]["vaultSource"],
        "VAULT_SOURCE_SEMANTIC_INVARIANCE"
    );
    Ok(())
}
// Additional LB010 controls; the frozen live-set invariant above is unchanged.
use meta_secret_core::node::{
    common::model::{
        device::common::DeviceName, meta_pass::MetaPasswordId, user::common::UserMembership,
    },
    db::events::object_id::Next,
};
use sources::vault_context;

fn witnessed_deleted_orders(original: &VaultObject) -> Result<[VaultObject; 2]> {
    ensure!(
        original.0.value.deleted_secrets.len() == 2,
        "TWO_DELETED_SECRETS_SETUP_HOLD"
    );
    let bytes =
        serde_json::to_vec(original).map_err(|_| anyhow::anyhow!("VAULT_SERIALIZE_SETUP_HOLD"))?;
    let view: Value =
        serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("VAULT_VIEW_SETUP_HOLD"))?;
    let order = &view["value"]["deletedSecrets"];
    ensure!(
        order.as_array().is_some_and(|items| items.len() == 2),
        "SET_ORDER_SHAPE_SETUP_HOLD"
    );
    for _ in 0..128 {
        let candidate: VaultObject = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("VAULT_REHYDRATE_SETUP_HOLD"))?;
        ensure!(
            candidate == *original && candidate.obj_id() == original.obj_id(),
            "TYPED_SOURCE_SETUP_HOLD"
        );
        let candidate_view = serde_json::to_value(&candidate)
            .map_err(|_| anyhow::anyhow!("CANDIDATE_VIEW_SETUP_HOLD"))?;
        if candidate_view["value"]["deletedSecrets"] != *order {
            return Ok([original.clone(), candidate]);
        }
    }
    bail!("SET_ORDER_WITNESS_SETUP_HOLD")
}

async fn field_fixture(
    delete_first: bool,
) -> Result<(
    fixture::Recovery,
    MetaPasswordId,
    MetaPasswordId,
    VaultObject,
)> {
    let mut r = fixture::Recovery::new()
        .await
        .map_err(|_| anyhow::anyhow!("RECOVERY_SETUP_HOLD"))?;
    let first =
        r.h.seed_secret("commitment-first")
            .await
            .map_err(|_| anyhow::anyhow!("FIRST_SEED_SETUP_HOLD"))?;
    let second =
        r.h.seed_secret("commitment-second")
            .await
            .map_err(|_| anyhow::anyhow!("SECOND_SEED_SETUP_HOLD"))?;
    if delete_first {
        r.h.vault = r.h.vault.clone().delete_secret(first.clone());
    }
    r.h.vault = r.h.vault.clone().delete_secret(second.clone());
    r.h.save_vault()
        .await
        .map_err(|_| anyhow::anyhow!("VAULT_SAVE_SETUP_HOLD"))?;
    let vault = PersistentVault::from(r.h.p.clone())
        .get_vault(r.h.vault.vault_name.clone())
        .await
        .map_err(|_| anyhow::anyhow!("VAULT_SOURCE_SETUP_HOLD"))?;
    Ok((r, first, second, vault))
}

#[tokio::test]
async fn equal_typed_vault_with_two_deleted_secret_orders_has_one_reveal_vault_source() -> Result<()>
{
    let (r, _, _, original) = field_fixture(true).await?;
    ensure!(
        original.0.value.secrets.contains(&r.pass),
        "TARGET_AVAILABLE_SETUP_HOLD"
    );
    let mut contexts = Vec::new();
    for vault in witnessed_deleted_orders(&original)? {
        contexts.push(
            vault_context(&r, &vault, false)
                .await?
                .context("ACCEPTED_PROOF_SETUP_HOLD")?,
        );
    }
    for field in [
        "version",
        "vaultName",
        "currentDeviceId",
        "localDistributionSource",
        "recoverySources",
        "claimScopeCommitment",
    ] {
        ensure!(
            !contexts[0][field].is_null() && contexts[0][field] == contexts[1][field],
            "UNRELATED_CONTEXT_SETUP_HOLD"
        );
    }
    ensure!(
        contexts[0]["vaultSource"] == contexts[1]["vaultSource"],
        "DELETED_VAULT_SOURCE_SEMANTIC_INVARIANCE"
    );
    Ok(())
}

#[tokio::test]
async fn vault_source_commits_full_set_elements_user_metadata_and_revision() -> Result<()> {
    for variant in 0..8 {
        let (mut r, live, dead, original) = field_fixture(false).await?;
        let before = vault_context(&r, &original, false)
            .await
            .with_context(|| format!("ORIGINAL_PROOF_VARIANT_{variant}"))?
            .context("ORIGINAL_PROOF_SETUP_HOLD")?;
        let mut changed = original.clone();
        match variant {
            0..=3 => {
                let (set, original_entry) = if variant < 2 {
                    (&mut changed.0.value.secrets, &live)
                } else {
                    (&mut changed.0.value.deleted_secrets, &dead)
                };
                let mut entry = original_entry.clone();
                if variant % 2 == 0 {
                    entry.name.push_str("-changed");
                } else {
                    entry.id = MetaPasswordId::build_from_str("changed-entry-id").id;
                }
                set.remove(original_entry);
                set.insert(entry);
            }
            4 | 5 => {
                let added =
                    r.h.seed_secret("additional-unrelated")
                        .await
                        .map_err(|_| anyhow::anyhow!("ADDITIONAL_SEED_SETUP_HOLD"))?;
                if variant == 5 {
                    r.h.vault = r.h.vault.clone().delete_secret(added);
                    r.h.save_vault()
                        .await
                        .map_err(|_| anyhow::anyhow!("VAULT_SAVE_SETUP_HOLD"))?;
                }
                changed = PersistentVault::from(r.h.p.clone())
                    .get_vault(r.h.vault.vault_name.clone())
                    .await
                    .map_err(|_| anyhow::anyhow!("VAULT_SOURCE_SETUP_HOLD"))?;
            }
            6 => {
                let member = changed
                    .0
                    .value
                    .users
                    .get_mut(r.h.target.device_id())
                    .context("MEMBER_SETUP_HOLD")?;
                let UserMembership::Member(member) = member else {
                    bail!("MEMBER_SETUP_HOLD")
                };
                member.user_data.device.device_name = DeviceName::from("changed-member-metadata");
            }
            _ => {
                changed.0.key = changed.0.key.next();
                r.h.p
                    .repo
                    .save(changed.clone())
                    .await
                    .map_err(|_| anyhow::anyhow!("NEXT_REVISION_SETUP_HOLD"))?;
            }
        }
        ensure!(
            changed != original && changed.0.value.secrets.contains(&r.pass),
            "REAL_NON_TARGET_DELTA_SETUP_HOLD"
        );
        let after = vault_context(&r, &changed, true)
            .await
            .with_context(|| format!("CHANGED_PROOF_VARIANT_{variant}"))?
            .context("CHANGED_PROOF_REQUIRED")?;
        ensure!(
            after["vaultSource"] != before["vaultSource"],
            "FULL_VAULT_FIELD_NOT_COMMITTED"
        );
    }
    Ok(())
}

#[test]
fn generic_canonical_json_preserves_ordered_arrays() -> Result<()> {
    let a = meta_secret_core::node::security::canonical_json(&[1, 2])
        .map_err(|_| anyhow::anyhow!("ORDERED_ARRAY_SETUP_HOLD"))?;
    let b = meta_secret_core::node::security::canonical_json(&[2, 1])
        .map_err(|_| anyhow::anyhow!("ORDERED_ARRAY_SETUP_HOLD"))?;
    ensure!(a != b, "GENERIC_ARRAY_ORDER_MUST_REMAIN_SIGNIFICANT");
    Ok(())
}
