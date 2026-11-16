//! Deliberate DB-corruption unit fixtures; no forged signed business lifecycle.
use super::fixture::*;
use anyhow::Result;
use meta_secret_core::node::{
    common::model::user::common::UserDataMember,
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::{
            kv_log_event::*,
            vault::{device_log_event::DeviceLogObject, vault_log_event::*},
        },
        repo::generic_db::*,
    },
};
async fn unexpected_kind(h: &H) -> Result<()> {
    let mut private = h.private().await?;
    private.0.key.obj_id = h.next(&h.a).await?;
    h.p.repo.save(private).await?;
    Ok(())
}
#[tokio::test]
async fn actual_server_tail_must_not_count_private_record_as_transport_progress() -> Result<()> {
    let h = H::new().await?;
    unexpected_kind(&h).await?;
    assert!(
        h.next_tail(&h.a).await.is_err(),
        "RED: server tail must validate Business-or-Noop type before progress"
    );
    Ok(())
}
#[tokio::test]
async fn persistent_device_log_tail_must_not_skip_unknown_record_kind() -> Result<()> {
    let h = H::new().await?;
    unexpected_kind(&h).await?;
    assert!(
        h.typed_tail(&h.a).await.is_err(),
        "RED: dedicated contiguous scanner rejects unexpected private kind"
    );
    Ok(())
}
#[tokio::test]
async fn business_record_with_foreign_inner_owner_is_not_valid_own_stream_progress() -> Result<()> {
    let h = H::new().await?;
    let key = KvKey::from(DeviceLogDescriptor::from(h.a.user_id()));
    let corrupt = DeviceLogObject(KvLogEvent {
        key,
        value: VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(
            RemoveMemberRequestEvent {
                sender: UserDataMember::from(h.b.user()),
                target: h.c.user().into(),
                share_transfers: vec![],
            },
        )),
    });
    h.p.repo.save(corrupt).await?;
    assert!(
        h.typed_tail(&h.a).await.is_err(),
        "RED: ArtifactId alone does not authenticate stream/Vault payload owner"
    );
    Ok(())
}
