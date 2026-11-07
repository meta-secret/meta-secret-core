use super::{recovery_intake_fixture::IntakeHarness, *};
use meta_secret_core::node::{
    api::SsRequest,
    db::{descriptors::shared_secret_descriptor::SsLogDescriptor, repo::generic_db::*},
};
use std::sync::atomic::Ordering;

async fn unavailable(
    h: &IntakeHarness<InMemKvLogEventRepo>,
    server: &ServerApp<InMemKvLogEventRepo>,
    request: SyncRequest,
) -> Result<()> {
    let before = h.repo.get_db().await;
    let result = server.handle_client_request(request).await;
    assert!(
        h.repo.get_db().await == before,
        "whole-scope intake audit mutated state before finding its conflict"
    );
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    let error = result
        .err()
        .context("RED: affected Prepared history was returned as healthy")?;
    assert_eq!(error.to_string(), "RecoveryRequestIntake.Unavailable");
    Ok(())
}
async fn malformed(
    h: &IntakeHarness<InMemKvLogEventRepo>,
    server: &ServerApp<InMemKvLogEventRepo>,
    request: SyncRequest,
) -> Result<()> {
    let before = h.repo.get_db().await;
    let published = h.publisher.0.load(Ordering::SeqCst);
    let error = server
        .handle_client_request(request)
        .await
        .err()
        .context("malformed tail accepted")?;
    assert_eq!(error.to_string(), "InvalidPublicReadTail");
    assert!(h.repo.get_db().await == before);
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), published);
    Ok(())
}
async fn valid_vault(h: &IntakeHarness<InMemKvLogEventRepo>, slot: usize) -> Result<SyncRequest> {
    let p = PersistentObject::new(h.repo.clone());
    let user = h.base.a.user();
    let mut tail = VaultTail {
        vault_status: p
            .find_free_id_by_obj_desc(VaultStatusDescriptor::from(user.user_id()))
            .await?,
        vault_log: p
            .find_free_id_by_obj_desc(VaultLogDescriptor::from(user.vault_name.clone()))
            .await?,
        vault: p
            .find_free_id_by_obj_desc(VaultDescriptor::from(user.vault_name.clone()))
            .await?,
    };
    match slot {
        0 => tail.vault_status = ArtifactId::from(VaultStatusDescriptor::from(user.user_id())),
        1 => tail.vault_log = ArtifactId::from(VaultLogDescriptor::from(user.vault_name.clone())),
        _ => tail.vault = ArtifactId::from(VaultDescriptor::from(user.vault_name.clone())),
    }
    Ok(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
        VaultRequest { sender: user, tail },
    ))))
}
async fn historical(route: usize) -> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    let mut head = h.prepared.clone();
    head.0.key = head.0.key.next();
    head.0.value.pending_recovery_request_intake = None;
    h.repo.save(head.clone()).await?;
    let request = match route {
        0 => h.tail(),
        1..=3 => {
            malformed(&h, &server, h.vault(route - 1, h.prepared.obj_id())).await?;
            valid_vault(&h, route - 1).await?
        }
        4 => h.ss(h.prepared.obj_id()),
        _ => h.ss(head.obj_id()),
    };
    unavailable(&h, &server, request).await
}
macro_rules! historical_case {
    ($name:ident, $route:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            historical($route).await
        }
    };
}
historical_case!(recovery_intake_nonhead_prepared_blocks_server_tail, 0);
historical_case!(
    recovery_intake_nonhead_prepared_blocks_generic_vault_status_tail,
    1
);
historical_case!(
    recovery_intake_nonhead_prepared_blocks_generic_vault_log_tail,
    2
);
historical_case!(
    recovery_intake_nonhead_prepared_blocks_generic_vault_tail,
    3
);
historical_case!(recovery_intake_nonhead_prepared_blocks_ss_history, 4);
historical_case!(
    recovery_intake_nonhead_prepared_blocks_ss_skipping_to_clean_head,
    5
);

async fn corruption(kind: u8) -> Result<()> {
    let mut h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    let missing_base = h.prepared.obj_id().next();
    let receipt = h
        .prepared
        .0
        .value
        .pending_recovery_request_intake
        .as_mut()
        .unwrap();
    match kind {
        0 => receipt.target_ss_log_artifact_id = receipt.target_ss_log_artifact_id.clone().next(),
        1 => receipt.version = 99,
        2 => receipt.source_body_hash = "00".repeat(32),
        3 => receipt.verified_vault_artifact_id = receipt.verified_vault_artifact_id.clone().next(),
        4 => receipt.base_ss_log_artifact_id = Some(missing_base),
        5 => receipt.requester_id = h.base.b.device_id().clone(),
        _ => {}
    }
    h.seed_prepared().await?;
    if kind == 6 {
        let mut duplicate = h.prepared.clone();
        duplicate.0.key = duplicate.0.key.next();
        h.repo.save(duplicate).await?;
    }
    if kind == 7 {
        let mut different = h.source.clone();
        different.0.value.status.statuses.insert(
            h.base.b.device_id().clone(),
            meta_secret_core::node::common::model::secret::SsDistributionStatus::Declined,
        );
        h.repo.save(different).await?;
    }
    if kind == 8 {
        let mut changed = h.prepared.clone();
        let mut extra = h.source.0.value.clone();
        extra.id = meta_secret_core::node::common::model::secret::ClaimId(
            meta_secret_core::crypto::utils::Id48bit::generate(),
        );
        extra.dist_claim_id.id = extra.id.clone();
        changed.0.value.claims.insert(extra.id.clone(), extra);
        h.repo.replace(changed).await?;
    }
    unavailable(&h, &server, h.tail()).await
}
macro_rules! corrupt_case {
    ($name:ident, $kind:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            corruption($kind).await
        }
    };
}
corrupt_case!(
    recovery_intake_copied_wrong_fixed_target_is_readonly_unavailable,
    0
);
corrupt_case!(recovery_intake_malformed_version_is_readonly_unavailable, 1);
corrupt_case!(
    recovery_intake_source_hash_conflict_is_readonly_unavailable,
    2
);
corrupt_case!(
    recovery_intake_changed_vault_context_is_readonly_unavailable,
    3
);
corrupt_case!(recovery_intake_missing_base_is_readonly_unavailable, 4);
corrupt_case!(
    recovery_intake_wrong_source_owner_is_readonly_unavailable,
    5
);
corrupt_case!(
    recovery_intake_two_physical_prepared_records_is_readonly_unavailable,
    6
);
corrupt_case!(
    recovery_intake_conflicting_raw_source_is_readonly_unavailable,
    7
);
corrupt_case!(
    recovery_intake_extra_target_mutation_fails_full_record_reconstruction,
    8
);

#[tokio::test]
async fn recovery_intake_beyond_gap_addressed_sslog_cannot_be_published() -> Result<()> {
    let mut h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.prepared.0.key.obj_id = h.prepared.obj_id().next().next();
    let id = h.prepared.obj_id();
    h.prepared
        .0
        .value
        .pending_recovery_request_intake
        .as_mut()
        .unwrap()
        .target_ss_log_artifact_id = id.clone();
    h.seed_prepared().await?;
    unavailable(&h, &server, h.ss(id)).await
}

#[tokio::test]
async fn recovery_intake_forged_sender_vault_does_not_hide_actual_addressed_owner() -> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    let SyncRequest::Read(mut request) = h.vault(0, h.prepared.obj_id()) else {
        unreachable!()
    };
    let ReadSyncRequest::Vault(ref mut vault) = *request else {
        unreachable!()
    };
    vault.sender.vault_name = "forged-unrelated-vault".into();
    malformed(&h, &server, SyncRequest::Read(request)).await?;
    let response = server
        .handle_client_request(valid_vault(&h, 0).await?)
        .await?;
    h.assert_settled().await?;
    super::recovery_intake_fixture::assert_clean(&response)
}

#[tokio::test]
async fn recovery_intake_cross_vault_tails_reject_before_independent_valid_repair() -> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    let mut other = h.prepared.clone();
    other.0.key = KvKey::from(SsLogDescriptor::from(
        meta_secret_core::node::common::model::vault::vault::VaultName::from("zz-other"),
    ));
    let other_id = other.obj_id();
    let receipt = other
        .0
        .value
        .pending_recovery_request_intake
        .as_mut()
        .unwrap();
    receipt.target_ss_log_artifact_id = other_id.clone();
    receipt.base_ss_log_artifact_id = Some(other_id.clone().next());
    for claim in other.0.value.claims.values_mut() {
        claim.vault_name = "zz-other".into();
    }
    h.repo.save(other.clone()).await?;
    let SyncRequest::Read(mut request) = h.vault(0, h.prepared.obj_id()) else {
        unreachable!()
    };
    let ReadSyncRequest::Vault(ref mut vault) = *request else {
        unreachable!()
    };
    vault.tail.vault_log = other_id.clone();
    malformed(&h, &server, SyncRequest::Read(request)).await?;
    let response = server
        .handle_client_request(valid_vault(&h, 1).await?)
        .await?;
    h.assert_settled().await?;
    super::recovery_intake_fixture::assert_clean(&response)?;
    assert!(h.repo.find_one(other_id).await? == Some(other.to_generic()));
    Ok(())
}

async fn direct(ss: bool) -> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let _server = h.server().await?;
    h.seed_prepared().await?;
    let gateway = super::super::server_data_sync::ServerSyncGateway::new(
        Arc::new(PersistentObject::new(h.repo.clone())),
        h.publisher.clone(),
    );
    if ss {
        h.repo
            .save(
                super::recovery_intake_ordering::response(&h, true)?
                    .event()?
                    .clone(),
            )
            .await?;
    }
    let before = h.repo.get_db().await;
    let result = if ss {
        gateway
            .ss_replication(
                SsRequest {
                    sender: h.base.a.user(),
                    ss_log: h.prepared.obj_id(),
                },
                h.base.c.device_id().clone(),
            )
            .await
    } else {
        let SyncRequest::Read(request) = valid_vault(&h, 0).await? else {
            unreachable!()
        };
        let ReadSyncRequest::Vault(request) = *request else {
            unreachable!()
        };
        gateway.vault_replication(request).await
    };
    assert!(
        h.repo.get_db().await == before,
        "RED: direct builder performed status/delivery mutation before Prepared guard"
    );
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), 0);
    let error = result
        .err()
        .context("RED: direct response builder serialized Prepared as healthy data")?;
    assert_eq!(error.to_string(), "RecoveryRequestIntake.Unavailable");
    Ok(())
}
#[tokio::test]
async fn recovery_intake_direct_vault_builder_fails_before_status_mutation() -> Result<()> {
    direct(false).await
}
#[tokio::test]
async fn recovery_intake_direct_ss_builder_fails_before_delivery_or_prepared_publication()
-> Result<()> {
    direct(true).await
}

#[tokio::test]
async fn recovery_intake_same_physical_receipt_addressed_and_in_history_repairs_once_and_returns_clean_data()
-> Result<()> {
    let h = IntakeHarness::new(Arc::new(InMemKvLogEventRepo::default())).await?;
    let server = h.server().await?;
    h.seed_prepared().await?;
    let id = h.prepared.obj_id();
    let request = SyncRequest::Read(Box::new(ReadSyncRequest::Vault(VaultRequest {
        sender: h.base.a.user(),
        tail: VaultTail {
            vault_status: id.clone(),
            vault_log: id.clone(),
            vault: id,
        },
    })));
    malformed(&h, &server, request).await?;
    // This valid Ss route encounters the same physical Prepared via addressed and history scans.
    let request = h.ss(h.prepared.obj_id());
    let response = server.handle_client_request(request.clone()).await?;
    h.assert_settled().await?;
    super::recovery_intake_fixture::assert_clean(&response)?;
    assert_eq!(h.log().await?.0.value.claims.len(), 1);
    let before = h.repo.get_db().await;
    let published = h.publisher.0.load(Ordering::SeqCst);
    let response = server.handle_client_request(request).await?;
    h.assert_settled().await?;
    super::recovery_intake_fixture::assert_clean(&response)?;
    assert!(h.repo.get_db().await == before);
    assert_eq!(h.publisher.0.load(Ordering::SeqCst), published);
    Ok(())
}
