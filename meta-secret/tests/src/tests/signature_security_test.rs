use crate::fixture::ExtendedFixtureRegistry;
use anyhow::Result;
use meta_secret_core::crypto::encoding::base64::Base64Text;
use meta_secret_core::node::api::{
    SignedAction, SignedActionPayload, SyncRequest, WriteSyncRequest,
};
use meta_secret_core::node::common::model::IdString;
use meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
use meta_secret_core::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use meta_secret_core::node::db::objects::persistent_device_log::PersistentDeviceLog;
use meta_secret_core::node::db::repo::persistent_credentials::PersistentCredentials;
use meta_secret_core::node::security::sign_event;

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
