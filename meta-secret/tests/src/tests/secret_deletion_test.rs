use crate::fixture::ExtendedFixtureRegistry;
use anyhow::{Result, bail};
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::SsDistributionId;
use meta_secret_core::node::common::model::user::common::{UserDataMember, UserMembership};
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::db::actions::vault::vault_action::ServerVaultAction;
use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use meta_secret_core::node::db::events::object_id::Next;
use meta_secret_core::node::db::events::shared_secret_event::SsWorkflowObject;
use meta_secret_core::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, VaultActionEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
};
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::db::objects::persistent_vault::PersistentVault;
use meta_secret_core::node::db::repo::generic_db::SaveCommand;

#[tokio::test]
async fn deletion_tombstone_blocks_server_share_delivery_even_when_cleanup_was_interrupted()
-> Result<()> {
    use meta_secret_core::node::api::SsRequest;
    use meta_secret_core::node::common::model::meta_pass::SecurePassInfo;
    use meta_secret_core::node::db::descriptors::shared_secret_descriptor::SsLogDescriptor;
    use meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent;
    use meta_secret_core::node::db::events::object_id::ArtifactId;
    use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
    use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use meta_secret_core::secret::MetaDistributor;
    use meta_server_node::server::server_data_sync::ServerSyncGateway;
    use std::sync::Arc;

    let fixture = FixtureRegistry::empty();
    let member = fixture.state.vault_data.client_vault_member;
    let creds = fixture.state.user_creds.client;
    let pass = MetaPasswordId::build_from_str("interrupted-delete");
    let p_obj = fixture.state.p_obj.server;
    MetaDistributor {
        p_obj: p_obj.clone(),
        user_creds: Arc::new(creds.clone()),
        vault_member: member.clone(),
    }
    .distribute(
        member.clone(),
        SecurePassInfo::from(PlainPassInfo {
            pass_id: pass.clone(),
            pass: "test-only-value".into(),
        }),
    )
    .await?;
    let p_ss = PersistentSharedSecret::from(p_obj.clone());
    let claim = p_ss
        .find_ss_device_log_tail_event(creds.device_id().clone())
        .await?
        .unwrap()
        .to_distribution_request();
    p_ss.save_ss_log_event(claim).await?;
    let mut object = VaultObject::sign_up(creds.vault_name.clone(), member.member.clone());
    object.0.value = member
        .vault
        .add_secret(pass.clone())
        .delete_secret(pass.clone());
    p_obj.repo.save(object).await?;
    let gateway = ServerSyncGateway::from(p_obj);
    let events = gateway
        .ss_replication(
            SsRequest {
                sender: fixture.state.user_creds.client_b.user(),
                ss_log: ArtifactId::from(SsLogDescriptor::from(creds.vault_name)),
            },
            fixture.state.device_creds.server.device.device_id,
        )
        .await?;
    for event in events {
        match event {
            GenericKvLogEvent::SsWorkflow(workflow) => assert_ne!(
                workflow.to_distribution_data()?.claim_id.pass_id,
                pass,
                "tombstones must suppress stale ciphertext delivery"
            ),
            GenericKvLogEvent::SsLog(log) => assert!(
                log.to_data()
                    .claims
                    .values()
                    .all(|claim| claim.dist_claim_id.pass_id != pass)
            ),
            _ => {}
        }
    }
    Ok(())
}

#[tokio::test]
async fn deletion_cleanup_sanitizes_every_removal_history_without_removing_metadata() -> Result<()>
{
    use meta_secret_core::node::common::model::user::common::{
        UserDataOutsider, UserDataOutsiderStatus,
    };
    use meta_secret_core::node::db::descriptors::vault_descriptor::{
        DeviceLogDescriptor, VaultLogDescriptor,
    };
    use meta_secret_core::node::db::events::vault::vault_log_event::{
        RemoveMemberDecision, RemoveMemberEvent, RemoveMemberRequestEvent,
        RemoveMemberShareTransfer, VaultLogObject,
    };
    use meta_secret_core::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    let fixture = FixtureRegistry::empty();
    let member = fixture.state.vault_data.client_vault_member;
    let a = member.member;
    let b = UserDataMember::from(fixture.state.user_creds.client_b.user());
    let c = UserDataMember::from(fixture.state.user_creds.vd.user());
    let pass = MetaPasswordId::build_from_str("deleted-history");
    let other = MetaPasswordId::build_from_str("retained-history");
    let transfer = |pass_id, encrypted_share: &str| RemoveMemberShareTransfer {
        pass_id,
        receiver: b.user_data.device.device_id.clone(),
        claim_id: None,
        encrypted_share: encrypted_share.into(),
    };
    let request = RemoveMemberRequestEvent {
        sender: a.clone(),
        target: c.clone(),
        share_transfers: vec![
            transfer(pass.clone(), "deleted-ciphertext"),
            transfer(other.clone(), "retained-ciphertext"),
        ],
    };
    let update = RemoveMemberEvent {
        request: request.clone(),
        sender: b,
        target: c.clone(),
        new_status: UserMembership::Outsider(UserDataOutsider {
            user_data: c.user_data,
            status: UserDataOutsiderStatus::Declined,
        }),
        decision: RemoveMemberDecision::Accepted,
        share_transfers: request.share_transfers.clone(),
    };
    let p_obj = fixture.state.p_obj.client;
    let device_log = PersistentDeviceLog::from(p_obj.clone());
    device_log
        .save_remove_member_request(request.clone())
        .await?;
    device_log.save_remove_member_update(update.clone()).await?;
    let mut log = VaultLogObject::create(a.clone());
    log.0.value = log
        .0
        .value
        .request(VaultActionRequestEvent::RemoveMember(request));
    p_obj.repo.save(log.clone()).await?;
    log.0.key.obj_id = log.0.key.obj_id.next();
    log.0.value = log
        .0
        .value
        .apply(VaultActionUpdateEvent::RemoveMember(update));
    p_obj.repo.save(log).await?;
    let before = p_obj.repo.get_db().await;
    let vault = member.vault.add_secret(other).delete_secret(pass);
    let cleanup = PersistentSharedSecret::from(p_obj.clone());
    cleanup.purge_deleted_secrets(&vault).await?;
    cleanup.purge_deleted_secrets(&vault).await?;
    let after = p_obj.repo.get_db().await;
    assert_eq!(before.len(), after.len());
    assert!(
        before.keys().all(|id| after.contains_key(id)),
        "metadata stream IDs must survive sanitization"
    );
    for event in after.values() {
        let serialized = serde_json::to_string(event)?;
        assert!(!serialized.contains("deleted-ciphertext"));
        assert!(serialized.contains("retained-ciphertext"));
    }
    assert_eq!(
        p_obj
            .get_object_events_from_beginning(DeviceLogDescriptor::from(a.user_data.user_id()))
            .await?
            .len(),
        1
    );
    assert_eq!(
        p_obj
            .get_object_events_from_beginning(VaultLogDescriptor::from(vault.vault_name))
            .await?
            .len(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn deletion_signed_protocol_rejects_forgery_replay_outsiders_and_self_approval() -> Result<()>
{
    use meta_secret_core::node::api::{
        SignedAction, SignedActionPayload, SyncRequest, WriteSyncRequest,
    };
    use meta_secret_core::node::common::model::user::common::UserMembership;
    use meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
    use meta_secret_core::node::db::events::generic_log_event::{
        GenericKvLogEvent, ToGenericEvent,
    };
    use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
    use meta_secret_core::node::db::events::vault::vault_log_event::{
        DeleteSecretDecision, DeleteSecretRequest, VaultActionEvent, VaultActionUpdateEvent,
        VaultLogObject,
    };
    use meta_secret_core::node::security::sign_event;

    let registry = ExtendedFixtureRegistry::extended().await?;
    let a = registry.state.base.empty.user_creds.client.clone();
    let b = registry.state.base.empty.user_creds.client_b.clone();
    let c = registry.state.base.empty.user_creds.vd.clone();
    let pass = MetaPasswordId::build_from_str("signed-delete");
    let mut object = VaultObject::sign_up(a.vault_name.clone(), UserDataMember::from(a.user()));
    object.0.value = object
        .0
        .value
        .add_secret(pass.clone())
        .update_membership(UserMembership::Member(UserDataMember::from(b.user())));
    // C remains an outsider for the first attempted signed request.
    registry
        .state
        .server_node
        .p_obj
        .repo
        .save(object.clone())
        .await?;
    registry
        .state
        .server_node
        .p_obj
        .repo
        .save(VaultLogObject::create(UserDataMember::from(a.user())))
        .await?;
    let server = &registry.state.server_app.server_app;
    let outsider_request = DeleteSecretRequest::new(
        UserDataMember::from(c.user()),
        pass.clone(),
        &object.0.value,
    );
    PersistentDeviceLog::from(registry.state.vd.p_obj.clone())
        .save_vault_action(
            &c.user(),
            VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(outsider_request)),
        )
        .await?;
    let event = registry
        .state
        .vd
        .p_obj
        .find_tail_event(DeviceLogDescriptor::from(c.user_id()))
        .await?
        .unwrap()
        .to_generic();
    let signed = sign_event(
        event,
        c.device_id().clone(),
        &c.device_creds.key_manager()?.dsa,
    )?;
    assert!(
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed
            ))))
            .await
            .is_err()
    );

    object.0.value = object
        .0
        .value
        .update_membership(UserMembership::Member(UserDataMember::from(c.user())));
    registry
        .state
        .server_node
        .p_obj
        .repo
        .replace(object.clone())
        .await?;
    let request = DeleteSecretRequest::new(
        UserDataMember::from(a.user()),
        pass.clone(),
        &object.0.value,
    );
    let log = PersistentDeviceLog::from(registry.state.client.p_obj.clone());
    log.save_vault_action(
        &a.user(),
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(request.clone())),
    )
    .await?;
    let event = registry
        .state
        .client
        .p_obj
        .find_tail_event(DeviceLogDescriptor::from(a.user_id()))
        .await?
        .unwrap()
        .to_generic();
    let valid = sign_event(
        event,
        a.device_id().clone(),
        &a.device_creds.key_manager()?.dsa,
    )?;
    let forged = SignedAction::sign(
        valid.payload.clone(),
        a.device_id().clone(),
        valid.stream.clone(),
        valid.nonce,
        &b.device_creds.key_manager()?.dsa,
    )?;
    assert!(
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                forged
            ))))
            .await
            .is_err()
    );
    server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            valid.clone(),
        ))))
        .await?;
    assert!(
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(valid))))
            .await
            .is_err()
    );

    let self_decision = DeleteSecretDecision {
        request: request.clone(),
        sender: UserDataMember::from(a.user()),
        accepted: true,
    };
    log.save_vault_action(
        &a.user(),
        VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(self_decision)),
    )
    .await?;
    let event = registry
        .state
        .client
        .p_obj
        .find_tail_event(DeviceLogDescriptor::from(a.user_id()))
        .await?
        .unwrap()
        .to_generic();
    let signed = sign_event(
        event,
        a.device_id().clone(),
        &a.device_creds.key_manager()?.dsa,
    )?;
    assert!(
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed
            ))))
            .await
            .is_err()
    );
    assert!(
        registry
            .state
            .server_node
            .p_vault
            .get_vault(a.vault_name.clone())
            .await?
            .to_data()
            .secrets
            .contains(&pass)
    );

    let receiver_log = PersistentDeviceLog::from(registry.state.base.empty.p_obj.client_b.clone());
    receiver_log
        .save_vault_action(
            &b.user(),
            VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request,
                sender: UserDataMember::from(b.user()),
                accepted: true,
            })),
        )
        .await?;
    let event = registry
        .state
        .base
        .empty
        .p_obj
        .client_b
        .find_tail_event(DeviceLogDescriptor::from(b.user_id()))
        .await?
        .unwrap()
        .to_generic();
    let signed = sign_event(
        event,
        b.device_id().clone(),
        &b.device_creds.key_manager()?.dsa,
    )?;
    let mut tampered = signed.clone();
    if let SignedActionPayload::Event(GenericKvLogEvent::DeviceLog(object)) = &mut tampered.payload
        && let VaultActionEvent::Update(VaultActionUpdateEvent::DeleteSecret(decision)) =
            &mut object.0.value
    {
        decision.accepted = false;
    }
    assert!(
        server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                tampered
            ))))
            .await
            .is_err()
    );
    server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            signed,
        ))))
        .await?;
    assert!(
        registry
            .state
            .server_node
            .p_vault
            .get_vault(a.vault_name.clone())
            .await?
            .to_data()
            .deleted_secrets
            .contains(&pass)
    );
    Ok(())
}

#[tokio::test]
async fn deletion_sync_removes_offline_full_copy_and_cannot_be_undone_by_stale_upload() -> Result<()>
{
    let registry = ExtendedFixtureRegistry::extended().await?;
    let client = &registry.state.client.client_service;
    let receiver = &registry.state.vd.client_service;
    let vault_name = registry.state.client.user.vault_name();
    for service in [client, receiver] {
        let state = service.get_app_state().await?;
        service
            .handle_client_request(state, GenericAppStateRequest::SignUp(vault_name.clone()))
            .await?;
    }
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = client.get_app_state().await?
    else {
        bail!("Owner must be a member");
    };
    let join = member
        .vault_events
        .requests
        .into_iter()
        .find_map(|event| match event {
            VaultActionRequestEvent::JoinCluster(join) => Some(join),
            _ => None,
        })
        .expect("receiver JOIN");
    registry
        .state
        .client
        .orchestrator
        .update_membership(join, JoinActionUpdate::Accept)
        .await?;
    client.get_app_state().await?;
    receiver.get_app_state().await?;

    let pass_id = MetaPasswordId::build_from_str("offline-delete");
    client
        .handle_client_request(
            client.get_app_state().await?,
            GenericAppStateRequest::ClusterDistribution(PlainPassInfo {
                pass_id: pass_id.clone(),
                pass: "test-only-value".into(),
            }),
        )
        .await?;
    receiver.get_app_state().await?;
    let desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass_id.clone(),
        receiver: registry.state.vd.device_id(),
    });
    assert!(
        registry
            .state
            .vd
            .p_obj
            .find_tail_event(desc.clone())
            .await?
            .is_some()
    );
    let Some(SsWorkflowObject::Distribution(mut revision)) = registry
        .state
        .vd
        .p_obj
        .find_tail_event(desc.clone())
        .await?
    else {
        bail!("Distribution expected");
    };
    revision.key.obj_id = revision.key.obj_id.next();
    registry
        .state
        .vd
        .p_obj
        .repo
        .save(SsWorkflowObject::Distribution(revision))
        .await?;

    // Receiver is offline while the owner deletes; its stale write is queued.
    PersistentDeviceLog::from(registry.state.vd.p_obj.clone())
        .save_add_meta_pass_request(AddMetaPassEvent {
            sender: UserDataMember::from(registry.state.vd.user.clone()),
            meta_pass_id: pass_id.clone(),
        })
        .await?;
    client
        .handle_client_request(
            client.get_app_state().await?,
            GenericAppStateRequest::DeleteSecret(pass_id.clone()),
        )
        .await?;
    assert!(
        registry
            .state
            .vd
            .p_obj
            .find_tail_event(desc.clone())
            .await?
            .is_some(),
        "offline copy remains until reconnect"
    );
    let ApplicationState::Vault(VaultFullInfo::Member(receiver_state)) =
        receiver.get_app_state().await?
    else {
        bail!("Receiver remains a member");
    };
    assert!(receiver_state.member.vault.secrets.is_empty());
    assert!(
        receiver_state
            .member
            .vault
            .deleted_secrets
            .contains(&pass_id)
    );
    assert!(
        receiver_state
            .ss_claims
            .claims
            .values()
            .all(|claim| claim.dist_claim_id.pass_id != pass_id)
    );
    assert!(
        registry
            .state
            .vd
            .p_obj
            .find_tail_event(desc)
            .await?
            .is_none()
    );
    for repo in [
        &registry.state.client.p_obj.repo,
        &registry.state.vd.p_obj.repo,
        &registry.state.server_node.p_obj.repo,
    ] {
        assert!(repo.get_db().await.values().all(|event| match event {
            meta_secret_core::node::db::events::generic_log_event::GenericKvLogEvent::SsWorkflow(workflow) => workflow.clone().to_distribution_data().map_or(true, |data| data.claim_id.pass_id.id != pass_id.id),
            _ => true,
        }), "deleted ciphertext must be absent from every workflow revision");
    }
    assert!(
        registry
            .state
            .server_node
            .p_vault
            .get_vault(vault_name.clone())
            .await?
            .to_data()
            .secrets
            .is_empty()
    );

    // Recreating the same display name uses a fresh ID; old tombstones survive.
    client
        .handle_client_request(
            client.get_app_state().await?,
            GenericAppStateRequest::ClusterDistribution(PlainPassInfo {
                pass_id: pass_id.clone(),
                pass: "new-test-only-value".into(),
            }),
        )
        .await?;
    let recreated = client.resolve_secret(&pass_id.name).await?;
    assert_ne!(recreated.id, pass_id.id);
    receiver.get_app_state().await?;
    assert_eq!(receiver.resolve_secret(&pass_id.name).await?, recreated);
    let new_desc = SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: recreated,
        receiver: registry.state.vd.device_id(),
    });
    assert!(
        registry
            .state
            .vd
            .p_obj
            .find_tail_event(new_desc)
            .await?
            .is_some()
    );
    assert!(
        client
            .handle_client_request(
                client.get_app_state().await?,
                GenericAppStateRequest::ClusterDistribution(PlainPassInfo {
                    pass_id: pass_id.clone(),
                    pass: "duplicate-test-only-value".into(),
                })
            )
            .await
            .is_err(),
        "a recreated display name must not create two active Secret IDs"
    );
    Ok(())
}

#[tokio::test]
async fn deletion_server_authorizes_one_two_and_three_device_flows() -> Result<()> {
    use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
    use meta_secret_core::node::db::events::vault::vault_log_event::{
        DeleteSecretDecision, DeleteSecretRequest, VaultLogObject,
    };
    for count in 1..=3 {
        let fixture = FixtureRegistry::empty();
        let a = UserDataMember::from(fixture.state.user_creds.client.user());
        let b = UserDataMember::from(fixture.state.user_creds.client_b.user());
        let c = UserDataMember::from(fixture.state.user_creds.vd.user());
        let pass = MetaPasswordId::build_from_str("delete-server");
        let mut object = VaultObject::sign_up(a.user_data.vault_name(), a.clone());
        object.0.value = object.0.value.add_secret(pass.clone());
        if count >= 2 {
            object.0.value = object
                .0
                .value
                .update_membership(UserMembership::Member(b.clone()));
        }
        if count >= 3 {
            object.0.value = object
                .0
                .value
                .update_membership(UserMembership::Member(c.clone()));
        }
        let request = DeleteSecretRequest::new(a.clone(), pass.clone(), &object.0.value);
        let p_obj = fixture.state.p_obj.server.clone();
        p_obj.repo.save(object).await?;
        p_obj.repo.save(VaultLogObject::create(a.clone())).await?;
        let server = ServerVaultAction {
            p_obj: p_obj.clone(),
            server_device: fixture.state.device_creds.server.device.clone(),
        };
        server
            .do_processing(VaultActionEvent::Request(
                VaultActionRequestEvent::DeleteSecret(request.clone()),
            ))
            .await?;
        let p_vault = PersistentVault::from(p_obj.clone());
        if count == 3 {
            assert!(
                p_vault
                    .get_vault(a.user_data.vault_name())
                    .await?
                    .to_data()
                    .secrets
                    .contains(&pass)
            );
            let self_approval = DeleteSecretDecision {
                request: request.clone(),
                sender: a.clone(),
                accepted: true,
            };
            assert!(
                server
                    .do_processing(VaultActionEvent::Update(
                        VaultActionUpdateEvent::DeleteSecret(self_approval)
                    ))
                    .await
                    .is_err()
            );
            server
                .do_processing(VaultActionEvent::Update(
                    VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                        request: request.clone(),
                        sender: b,
                        accepted: true,
                    }),
                ))
                .await?;
        }
        let vault = p_vault.get_vault(a.user_data.vault_name()).await?.to_data();
        assert!(!vault.secrets.contains(&pass));
        assert!(vault.deleted_secrets.contains(&pass));
        assert!(
            p_vault
                .get_vault_log_artifact(a.user_data.vault_name())
                .await?
                .0
                .value
                .deletion_results
                .iter()
                .any(|result| result.request == request && result.accepted)
        );
        server
            .do_processing(VaultActionEvent::Request(
                VaultActionRequestEvent::AddMetaPass(AddMetaPassEvent {
                    sender: a.clone(),
                    meta_pass_id: pass.clone(),
                }),
            ))
            .await?;
        assert!(
            !p_vault
                .get_vault(a.user_data.vault_name())
                .await?
                .to_data()
                .secrets
                .contains(&pass)
        );
    }
    Ok(())
}

#[tokio::test]
async fn deletion_first_decline_blocks_late_accept_and_wrong_request() -> Result<()> {
    use meta_secret_core::node::db::events::vault::vault_event::VaultObject;
    use meta_secret_core::node::db::events::vault::vault_log_event::{
        DeleteSecretDecision, DeleteSecretRequest, VaultLogObject,
    };
    let fixture = FixtureRegistry::empty();
    let a = UserDataMember::from(fixture.state.user_creds.client.user());
    let b = UserDataMember::from(fixture.state.user_creds.client_b.user());
    let c = UserDataMember::from(fixture.state.user_creds.vd.user());
    let pass = MetaPasswordId::build_from_str("delete-decline");
    let mut object = VaultObject::sign_up(a.user_data.vault_name(), a.clone());
    object.0.value = object
        .0
        .value
        .add_secret(pass.clone())
        .update_membership(UserMembership::Member(b.clone()))
        .update_membership(UserMembership::Member(c.clone()));
    let request = DeleteSecretRequest::new(a.clone(), pass.clone(), &object.0.value);
    let p_obj = fixture.state.p_obj.server.clone();
    p_obj.repo.save(object).await?;
    p_obj.repo.save(VaultLogObject::create(a.clone())).await?;
    let server = ServerVaultAction {
        p_obj: p_obj.clone(),
        server_device: fixture.state.device_creds.server.device.clone(),
    };
    server
        .do_processing(VaultActionEvent::Request(
            VaultActionRequestEvent::DeleteSecret(request.clone()),
        ))
        .await?;
    let mut wrong = request.clone();
    wrong.request_id = "00000000-0000-4000-8000-000000000001".to_string();
    server
        .do_processing(VaultActionEvent::Update(
            VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                request: wrong,
                sender: b.clone(),
                accepted: true,
            }),
        ))
        .await?;
    assert!(
        PersistentVault::from(p_obj.clone())
            .get_vault(a.user_data.vault_name())
            .await?
            .to_data()
            .secrets
            .contains(&pass)
    );
    for (sender, accepted) in [(b, false), (c, true)] {
        server
            .do_processing(VaultActionEvent::Update(
                VaultActionUpdateEvent::DeleteSecret(DeleteSecretDecision {
                    request: request.clone(),
                    sender,
                    accepted,
                }),
            ))
            .await?;
    }
    let vault = PersistentVault::from(p_obj.clone())
        .get_vault(a.user_data.vault_name())
        .await?
        .to_data();
    assert!(vault.secrets.contains(&pass));
    assert!(vault.deleted_secrets.is_empty());
    Ok(())
}
