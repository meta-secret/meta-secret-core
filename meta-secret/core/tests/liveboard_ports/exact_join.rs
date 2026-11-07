use super::fixtures::*;
use anyhow::{Context, Result};
use meta_secret_core::node::api::CanonicalDecisionInput;
use meta_secret_core::node::app::orchestrator::MetaOrchestrator;
use meta_secret_core::node::common::model::user::common::*;
use meta_secret_core::node::db::{
    actions::sign_up::join::JoinActionUpdate,
    descriptors::vault_descriptor::DeviceLogDescriptor,
    events::vault::{device_log_event::DeviceLogObject, vault_log_event::*},
    repo::generic_db::*,
};
use meta_secret_core::recover_from_shares;

pub const JOIN_ID: &str = "16f32e6e-8d50-4ea5-b332-269f45e9b36a";
const OLD_ID: &str = "bb97c97b-2a71-414f-a496-01fbc58a668a";

pub(super) async fn pending(h: &mut Harness) -> Result<JoinClusterEvent> {
    let candidate = h.target.user();
    h.vault = h
        .vault
        .clone()
        .update_membership(UserMembership::Outsider(UserDataOutsider {
            user_data: candidate.clone(),
            status: UserDataOutsiderStatus::Pending,
        }));
    h.save_vault().await?;
    let request = JoinClusterEvent {
        candidate,
        request_id: JOIN_ID.into(),
    };
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0
        .value
        .requests
        .insert(VaultActionRequestEvent::JoinCluster(request.clone()));
    log.0.value.join_results.insert(JoinResultRecord {
        request: JoinClusterEvent {
            request_id: OLD_ID.into(),
            ..request.clone()
        },
        outcome: JoinResultOutcome::Declined,
    });
    h.p.repo.replace(log).await?;
    Ok(request)
}
async fn updates(h: &Harness) -> Result<Vec<UpdateMembershipEvent>> {
    let events: Vec<DeviceLogObject> =
        h.p.get_object_events_from_beginning(DeviceLogDescriptor::from(h.approver.user_id()))
            .await?;
    Ok(events
        .into_iter()
        .filter_map(|e| match e.0.value {
            VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(u)) => Some(u),
            _ => None,
        })
        .collect())
}

#[tokio::test]
async fn exact_join_accept_returns_current_uuid_and_redistributes_a_real_copy_to_only_that_candidate()
-> Result<()> {
    let mut h = Harness::new(1).await?;
    let pass = h.seed_secret("join-real-material").await?;
    let request = pending(&mut h).await?;
    let receipt = h
        .service()
        .decide_join_request(CanonicalDecisionInput {
            request_id: JOIN_ID.into(),
            accepted: true,
        })
        .await
        .context("RED: exact JOIN must resolve and apply the current canonical UUID")?;
    assert_eq!(receipt.request_id.as_deref(), Some(JOIN_ID));
    assert_eq!(serde_json::to_value(&receipt)?["kind"], "join");
    let updates = updates(&h).await?;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].request, request);
    assert_eq!(updates[0].sender.user_data, h.approver.user());
    assert!(matches!(updates[0].update, UserMembership::Member(_)));
    let new_share = h.share(&pass, &h.target).await?;
    assert_eq!(new_share.share_blocks[0].config.threshold, 1);
    assert_eq!(recover_from_shares(vec![new_share])?.text, VALUE);
    let before_retry = h.p.repo.get_db().await;
    // A later canonical retirement is rejected; exact service cannot silently succeed or guess by candidate.
    let mut log = VaultLogObject::create(UserDataMember::from(h.approver.user()));
    log.0.value.join_results.insert(JoinResultRecord {
        request,
        outcome: JoinResultOutcome::Accepted,
    });
    h.p.repo.replace(log).await?;
    let settled = h.p.repo.get_db().await;
    assert!(before_retry != settled);
    assert!(
        h.service()
            .decide_join_request(CanonicalDecisionInput {
                request_id: JOIN_ID.into(),
                accepted: true
            })
            .await
            .is_err()
    );
    assert!(h.p.repo.get_db().await == settled);
    Ok(())
}

#[tokio::test]
async fn exact_join_decline_has_no_new_member_or_share_but_persists_the_exact_decision()
-> Result<()> {
    let mut h = Harness::new(1).await?;
    let pass = h.seed_secret("join-decline-material").await?;
    let request = pending(&mut h).await?;
    let receipt = h
        .service()
        .decide_join_request(CanonicalDecisionInput {
            request_id: JOIN_ID.into(),
            accepted: false,
        })
        .await
        .context("RED: exact JOIN Decline must queue the named request")?;
    assert_eq!(receipt.request_id.as_deref(), Some(JOIN_ID));
    let updates = updates(&h).await?;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].request, request);
    assert!(
        matches!(&updates[0].update, UserMembership::Outsider(o) if o.status == UserDataOutsiderStatus::Declined)
    );
    assert!(h.distribution(&pass, &h.target).await.is_err());
    assert!(!h.vault.is_member(h.target.device_id()));
    Ok(())
}

#[tokio::test]
async fn exact_join_stale_uuid_cannot_fall_back_to_identical_candidate_new_request() -> Result<()> {
    let mut h = Harness::new(1).await?;
    h.seed_secret("join-stale-material").await?;
    pending(&mut h).await?;
    let before = h.p.repo.get_db().await;
    for id in [OLD_ID, "", "not-a-uuid"] {
        assert!(
            h.service()
                .decide_join_request(CanonicalDecisionInput {
                    request_id: id.into(),
                    accepted: true
                })
                .await
                .is_err()
        );
        assert!(
            h.p.repo.get_db().await == before,
            "stale/invalid exact input must have no local effects"
        );
    }
    assert!(updates(&h).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn legacy_empty_join_id_wrapper_still_matches_the_canonical_candidate() -> Result<()> {
    let mut h = Harness::new(1).await?;
    let mut request = pending(&mut h).await?;
    request.request_id.clear();
    MetaOrchestrator {
        p_obj: h.p.clone(),
        user_creds: h.approver.clone(),
    }
    .update_membership(request, JoinActionUpdate::Decline)
    .await?;
    let updates = updates(&h).await?;
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].request.request_id, JOIN_ID);
    Ok(())
}
