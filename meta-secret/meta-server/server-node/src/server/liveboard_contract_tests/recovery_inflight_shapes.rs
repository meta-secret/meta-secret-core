//! These are pure source-classifier controls, not actual topology/UI E2E.
use super::super::recovery_intake_validation::initial_source;
use super::{recovery_intake_fixture::IntakeHarness, *};
use meta_secret_core::node::{
    common::model::{
        secret::SsDistributionStatus,
        vault::{vault::VaultMember, vault_data::VaultData},
    },
    db::events::shared_secret_event::SsDeviceLogObject,
};

#[tokio::test]
async fn recovery_inflight_classifier_retains_receiver_order_without_current_set_normalization()
-> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let vault = PersistentVault::from(Arc::new(PersistentObject::new(h.repo.clone())))
        .get_vault(h.base.a.vault_name.clone())
        .await?
        .0
        .value;
    let mut reversed = h.source.clone();
    reversed.0.value.receivers.reverse();
    assert!(initial_source(&vault, &reversed));
    assert_eq!(
        reversed.0.value.receivers,
        h.source
            .0
            .value
            .receivers
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
    );
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_classifier_empty_original_peers_remain_initial_after_topology_grows()
-> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let current = PersistentVault::from(Arc::new(PersistentObject::new(h.repo.clone())))
        .get_vault(h.base.a.vault_name.clone())
        .await?
        .0
        .value;
    let original = VaultData::from(member(&h.base.a))
        .add_secret(h.source.0.value.dist_claim_id.pass_id.clone());
    let claim = VaultMember {
        member: member(&h.base.a),
        vault: original,
    }
    .create_recovery_claim(h.source.0.value.dist_claim_id.pass_id.clone());
    assert!(claim.receivers.is_empty());
    let source = SsDeviceLogObject(KvLogEvent {
        key: h.source.0.key,
        value: claim,
    });
    assert!(
        initial_source(&current, &source),
        "RED: immutable empty initial shape does not become a snapshot when current peers grow"
    );
    Ok(())
}
#[tokio::test]
async fn recovery_inflight_classifier_malformed_shapes_gain_no_intake_authority_without_new_rejection_policy()
-> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let vault = PersistentVault::from(Arc::new(PersistentObject::new(h.repo.clone())))
        .get_vault(h.base.a.vault_name.clone())
        .await?
        .0
        .value;
    for kind in 0..7 {
        let mut source = h.source.clone();
        match kind {
            0 => source
                .0
                .value
                .receivers
                .push(source.0.value.receivers[0].clone()),
            1 => source.0.value.receivers.push(source.0.value.sender.clone()),
            2 => {
                source
                    .0
                    .value
                    .status
                    .statuses
                    .remove(&source.0.value.receivers[0]);
            }
            3 => {
                source
                    .0
                    .value
                    .status
                    .statuses
                    .insert(source.0.value.sender.clone(), SsDistributionStatus::Pending);
            }
            4 => {
                source.0.value.status.statuses.insert(
                    source.0.value.receivers[0].clone(),
                    SsDistributionStatus::Sent,
                );
            }
            5 => {
                source.0.value.dist_claim_id.id =
                    meta_secret_core::node::common::model::secret::ClaimId(
                        meta_secret_core::crypto::utils::Id48bit::generate(),
                    )
            }
            _ => source.0.key.obj_id = source.obj_id().next(),
        }
        if kind == 6 {
            source.0.key.obj_desc = h.base_log.0.key.obj_desc.clone();
        }
        assert!(
            !initial_source(&vault, &source),
            "malformed classifier case {kind} must not gain continuation authority; legacy acceptance is not changed here"
        );
    }
    Ok(())
}
