use crate::fixture::ExtendedFixtureRegistry;
use anyhow::Result;
use axum::{extract::State, routing::post, Json, Router};
use meta_secret_core::crypto::encoding::base64::Base64Text;
use meta_secret_core::node::api::{
    DataSyncResponse, ReadSyncRequest, SignedAction, SignedActionPayload, SsRecoveryCompletion,
    SyncRequest, WriteSyncRequest,
};
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;
use meta_secret_core::node::app::meta_app::meta_client_service::fixture::MetaClientServiceFixture;
use meta_secret_core::node::app::orchestrator::MetaOrchestrator;
use meta_secret_core::node::app::sync::api_url::ApiUrl;
use meta_secret_core::node::app::sync::sync_protocol::{HttpSyncProtocol, SyncProtocol};
use meta_secret_core::node::common::model::meta_pass::{MetaPasswordId, PlainPassInfo};
use meta_secret_core::node::common::model::secret::SsDistributionStatus;
use meta_secret_core::node::common::model::{ApplicationState, VaultFullInfo};
use meta_secret_core::node::db::actions::sign_up::join::JoinActionUpdate;
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use meta_secret_core::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent;
use meta_secret_core::node::db::in_mem_db::InMemKvLogEventRepo;
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::node::security::{sign_event, sign_recovery_completion};
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_server_node::server::server_app::{MetaServerDataTransfer, ServerApp};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

struct LocalHttpServer {
    protocol: Arc<HttpSyncProtocol>,
    server_task: JoinHandle<()>,
    http_task: JoinHandle<()>,
}

impl LocalHttpServer {
    async fn start(server: Arc<ServerApp<InMemKvLogEventRepo>>) -> Result<Self> {
        let data_transfer = server.get_data_transfer();
        let server_task = tokio::task::spawn_local(async move {
            if let Err(error) = server.run().await {
                eprintln!("test meta-server stopped: {error:?}");
            }
        });

        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port() as u32;
        let app = Router::new()
            .route("/meta_request", post(test_meta_request))
            .with_state(data_transfer);
        let http_task = tokio::task::spawn_local(async move {
            if let Err(error) = axum::serve(listener, app).await {
                eprintln!("test HTTP server stopped: {error:?}");
            }
        });

        Ok(Self {
            protocol: Arc::new(HttpSyncProtocol {
                api_url: ApiUrl::custom_dev("http://127.0.0.1", port),
            }),
            server_task,
            http_task,
        })
    }
}

impl Drop for LocalHttpServer {
    fn drop(&mut self) {
        self.server_task.abort();
        self.http_task.abort();
    }
}

async fn test_meta_request(
    State(data_transfer): State<Arc<MetaServerDataTransfer>>,
    Json(request): Json<SyncRequest>,
) -> Json<DataSyncResponse> {
    let response = match data_transfer.send_request(request).await {
        Ok(response) => response,
        Err(error) => DataSyncResponse::Error {
            msg: format!("test HTTP server request failed: {error:?}"),
        },
    };
    Json(response)
}

#[tokio::test]
async fn server_rejects_unsigned_forged_and_replayed_actions() -> Result<()> {
    let registry = ExtendedFixtureRegistry::extended().await?;
    let server = registry.state.server_app.server_app.clone();
    let client_creds = registry.state.base.empty.user_creds.client.clone();
    let client_b_creds = registry.state.base.empty.user_creds.client_b.clone();
    let client_p_obj = registry.state.base.empty.p_obj.client.clone();

    // Persist the local credentials used by the signing gateway in this direct
    // protocol test, then create a real bootstrap JOIN/CreateVault event.
    PersistentCredentials {
        p_obj: client_p_obj.clone(),
        master_key: registry
            .state
            .base
            .empty
            .device_creds
            .client_master_key
            .clone(),
    }
    .save_user_creds(client_creds.clone())
    .await?;
    PersistentDeviceLog::from(client_p_obj.clone())
        .save_create_vault_request(&client_creds.user())
        .await?;
    let event = client_p_obj
        .find_tail_event(DeviceLogDescriptor::from(client_creds.user_id()))
        .await?
        .expect("bootstrap event")
        .to_generic();
    let object_id = event.obj_id();
    let key_manager = client_creds.device_creds.key_manager()?;
    let valid = sign_event(
        event.clone(),
        client_creds.device_id().clone(),
        &key_manager.dsa,
    )?;

    server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            valid.clone(),
        ))))
        .await?;

    // A valid signature made by another device cannot impersonate the owner.
    let forged_key_manager = client_b_creds.device_creds.key_manager()?;
    let forged = SignedAction::sign(
        SignedActionPayload::Event(event.clone()),
        client_creds.device_id().clone(),
        object_id.fqdn.clone().id_str(),
        object_id.id.curr as u64,
        &forged_key_manager.dsa,
    )?;
    assert!(server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            forged
        ))))
        .await
        .is_err());

    // Empty signature is rejected even when all clear-text fields look valid.
    let unsigned = SignedAction {
        signer: client_creds.device_id().clone(),
        nonce: object_id.id.curr as u64 + 1,
        stream: object_id.fqdn.clone().id_str(),
        signature: Base64Text::from(""),
        payload: SignedActionPayload::Event(event),
    };
    assert!(server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            unsigned
        ))))
        .await
        .is_err());

    // Replaying the already accepted sequence is rejected before persistence.
    assert!(server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(valid))))
        .await
        .is_err());
    Ok(())
}

/// Test #20 security path: send forged recovery responses through the real
/// local HTTP server, not directly to Core or ServerApp.
#[tokio::test(flavor = "current_thread")]
async fn test20_http_server_rejects_forged_recovery_responses() -> Result<()> {
    tokio::task::LocalSet::new()
        .run_until(test20_http_server_rejects_forged_recovery_responses_impl())
        .await
}

async fn test20_http_server_rejects_forged_recovery_responses_impl() -> Result<()> {
    let registry = FixtureRegistry::base().await?;
    let server_fixture =
        crate::tests::meta_secret_test::fixture::ServerAppFixture::try_from(&registry)?;
    let http_server = LocalHttpServer::start(server_fixture.server_app).await?;
    let protocol = http_server.protocol.clone();
    let services = MetaClientServiceFixture::from(&registry.state, protocol.clone());
    let client_service = services.client.clone();
    let receiver_service = services.vd.clone();

    let client_creds = registry.state.empty.user_creds.client.clone();
    let client_user = client_creds.user();
    let receiver_user = registry.state.empty.user_creds.vd.user();
    let foreign_creds = registry.state.empty.user_creds.client_b.clone();
    let vault_name = client_user.vault_name();

    // Create a real two-device Vault through the client services and the
    // local HTTP server. The receiver is the only valid recovery responder.
    let client_state = client_service.build_service_state().await?;
    client_service
        .handle_client_request(
            client_state.app_state,
            GenericAppStateRequest::SignUp(vault_name.clone()),
        )
        .await?;

    let receiver_state = receiver_service.build_service_state().await?;
    receiver_service
        .handle_client_request(
            receiver_state.app_state,
            GenericAppStateRequest::SignUp(vault_name.clone()),
        )
        .await?;

    let client_state = client_service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(client_member)) = client_state else {
        anyhow::bail!("client did not become a Vault member");
    };
    let join_request = client_member
        .vault_events
        .requests
        .iter()
        .find_map(|event| match event {
            VaultActionRequestEvent::JoinCluster(request) => Some(request.clone()),
            VaultActionRequestEvent::AddMetaPass(_) => None,
        })
        .ok_or_else(|| anyhow::anyhow!("receiver join request was not found"))?;

    let client_orchestrator = MetaOrchestrator {
        p_obj: client_service.p_obj.clone(),
        user_creds: registry.state.empty.user_creds.client.clone(),
    };
    client_orchestrator
        .update_membership(join_request, JoinActionUpdate::Accept)
        .await?;

    for _ in 0..2 {
        services
            .sync_gateway
            .client_gw
            .sync(client_user.clone())
            .await?;
        services
            .sync_gateway
            .vd_gw
            .sync(receiver_user.clone())
            .await?;
    }

    let pass_id = MetaPasswordId::build_from_str("test20_http_security");
    let client_state = client_service.get_app_state().await?;
    client_service
        .handle_client_request(
            client_state,
            GenericAppStateRequest::ClusterDistribution(PlainPassInfo {
                pass_id: pass_id.clone(),
                pass: "test20-value".to_owned(),
            }),
        )
        .await?;

    for _ in 0..2 {
        services
            .sync_gateway
            .client_gw
            .sync(client_user.clone())
            .await?;
        services
            .sync_gateway
            .vd_gw
            .sync(receiver_user.clone())
            .await?;
    }

    let receiver_state = receiver_service.get_app_state().await?;
    receiver_service
        .handle_client_request(
            receiver_state,
            GenericAppStateRequest::Recover(pass_id.clone()),
        )
        .await?;

    let receiver_state = receiver_service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(receiver_member)) = receiver_state else {
        anyhow::bail!("receiver did not remain a Vault member");
    };
    let recovery_claim = receiver_member
        .ss_claims
        .claims
        .values()
        .find(|claim| {
            claim.dist_claim_id.pass_id == pass_id
                && claim.distribution_type
                    == meta_secret_core::node::common::model::secret::SecretDistributionType::Recover
        })
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("recovery claim was not created"))?;
    let recovery_id = recovery_claim
        .recovery_db_ids()
        .into_iter()
        .find(|id| id.distribution_id.receiver == client_user.device.device_id)
        .ok_or_else(|| anyhow::anyhow!("client receiver was not listed in recovery claim"))?;

    let send_completion = |action: SignedAction| async {
        protocol
            .send(SyncRequest::Read(Box::new(
                ReadSyncRequest::SsRecoveryCompletion(action),
            )))
            .await
    };

    // A valid signature from a non-member must not authorize a completion.
    let foreign_key_manager = foreign_creds.device_creds.key_manager()?;
    let foreign_action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name: vault_name.clone(),
            recovery_id: recovery_id.clone(),
            receiver_status: SsDistributionStatus::Sent,
        },
        foreign_creds.device_id().clone(),
        &foreign_key_manager.dsa,
    )?;
    assert!(matches!(
        send_completion(foreign_action).await?,
        DataSyncResponse::Error { .. }
    ));

    // The sender is a Vault member but is not the receiver named by the claim.
    let sender_creds = registry.state.empty.user_creds.vd.clone();
    let sender_key_manager = sender_creds.device_creds.key_manager()?;
    let sender_action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name: vault_name.clone(),
            recovery_id: recovery_id.clone(),
            receiver_status: SsDistributionStatus::Sent,
        },
        sender_creds.device_id().clone(),
        &sender_key_manager.dsa,
    )?;
    assert!(matches!(
        send_completion(sender_action).await?,
        DataSyncResponse::Error { .. }
    ));

    // A legitimate receiver signature cannot be redirected to another claim.
    let receiver_key_manager = client_creds.device_creds.key_manager()?;
    let mut wrong_claim_id = recovery_id.clone();
    wrong_claim_id.claim_id.id = meta_secret_core::node::common::model::secret::ClaimId::from(
        meta_secret_core::crypto::utils::Id48bit::generate(),
    );
    let wrong_claim_action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name: vault_name.clone(),
            recovery_id: wrong_claim_id,
            receiver_status: SsDistributionStatus::Sent,
        },
        client_creds.device_id().clone(),
        &receiver_key_manager.dsa,
    )?;
    assert!(matches!(
        send_completion(wrong_claim_action).await?,
        DataSyncResponse::Error { .. }
    ));

    // The same signed response must not be accepted under another Vault ID.
    let wrong_vault_action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name: meta_secret_core::node::common::model::vault::vault::VaultName::generate(),
            recovery_id: recovery_id.clone(),
            receiver_status: SsDistributionStatus::Sent,
        },
        client_creds.device_id().clone(),
        &receiver_key_manager.dsa,
    )?;
    assert!(matches!(
        send_completion(wrong_vault_action).await?,
        DataSyncResponse::Error { .. }
    ));

    // Rejected attempts must leave the real claim pending, and the valid
    // receiver response must still complete it afterwards.
    let receiver_state = receiver_service.get_app_state().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(receiver_member)) = receiver_state else {
        anyhow::bail!("receiver state disappeared after rejected responses");
    };
    let pending_claim = receiver_member
        .ss_claims
        .claims
        .get(&recovery_claim.id)
        .ok_or_else(|| anyhow::anyhow!("recovery claim disappeared after rejected responses"))?;
    assert_eq!(
        pending_claim.status.get(&client_user.device.device_id),
        Some(&SsDistributionStatus::Pending),
        "forged responses must not change receiver status"
    );

    let valid_action = sign_recovery_completion(
        SsRecoveryCompletion {
            vault_name,
            recovery_id,
            receiver_status: SsDistributionStatus::Sent,
        },
        client_creds.device_id().clone(),
        &receiver_key_manager.dsa,
    )?;
    assert!(matches!(
        send_completion(valid_action).await?,
        DataSyncResponse::Data(_)
    ));

    Ok(())
}
