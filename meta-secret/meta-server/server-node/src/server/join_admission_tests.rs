//! Actual cold ServerApp JoinAdmission route; no init/bootstrap/replication.
use super::server_app::ServerApp;
use anyhow::Result;
use meta_secret_core::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        api::{DataSyncResponse, ReadSyncRequest, SyncRequest},
        common::model::{
            device::{common::DeviceName, device_creds::DeviceCredsBuilder},
            user::common::{
                UserData, UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
            },
            vault::vault_data::VaultData,
        },
        db::{
            descriptors::vault_descriptor::VaultDescriptor,
            events::{
                kv_log_event::{KvKey, KvLogEvent},
                vault::vault_event::VaultObject,
            },
            in_mem_db::InMemKvLogEventRepo,
            repo::generic_db::SaveCommand,
        },
    },
};
use std::sync::Arc;

fn unavailable() -> DataSyncResponse {
    DataSyncResponse::Error {
        msg: "Join admission unavailable".into(),
    }
}
fn fixture(count: usize) -> (VaultData, UserData) {
    let state = FixtureRegistry::empty().state;
    let mut vault = VaultData::from(UserDataMember::from(state.user_creds.client.user()));
    for user in [state.user_creds.client_b.user(), state.user_creds.vd.user()]
        .into_iter()
        .take(count - 1)
    {
        vault = vault.update_membership(UserMembership::Member(UserDataMember::from(user)));
    }
    let candidate = UserData {
        vault_name: vault.vault_name.clone(),
        device: DeviceCredsBuilder::generate()
            .build(DeviceName::generate())
            .creds
            .device,
    };
    (vault, candidate)
}
async fn cold_response(
    vault: Option<VaultData>,
    candidate: UserData,
    expected: DataSyncResponse,
) -> Result<()> {
    let repo = Arc::new(InMemKvLogEventRepo::default());
    if let Some(vault) = vault {
        repo.save(VaultObject(KvLogEvent {
            key: KvKey::from(VaultDescriptor::from(candidate.vault_name())),
            value: vault,
        }))
        .await?;
    }
    let before = repo.get_db().await;
    let master = FixtureRegistry::empty()
        .state
        .device_creds
        .server_master_key;
    let server = ServerApp::new(repo.clone(), master)?;
    // Deliberately never call init(): the admission read must not create server credentials.
    let response = server
        .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::JoinAdmission(
            candidate,
        ))))
        .await?;
    assert_eq!(response, expected);
    assert_eq!(
        repo.get_db().await,
        before,
        "cold admission must write zero objects, including bootstrap/status/logs"
    );
    let shape = serde_json::to_value(response)?;
    assert!(
        shape == serde_json::json!("empty")
            || shape == serde_json::json!({"error":{"msg":"Join admission unavailable"}})
            || shape == serde_json::json!({"error":{"msg":"Vault supports at most 3 devices"}}),
        "admission response contains only a fixed public outcome"
    );
    Ok(())
}

#[tokio::test]
async fn cold_join_admission_allows_below_capacity_and_returns_fixed_full_outcome() -> Result<()> {
    for count in [1, 2, 3] {
        for status in [
            UserDataOutsiderStatus::NonMember,
            UserDataOutsiderStatus::Declined,
        ] {
            let (vault, candidate) = fixture(count);
            let vault = vault.update_membership(UserMembership::Outsider(UserDataOutsider {
                user_data: candidate.clone(),
                status,
            }));
            cold_response(
                Some(vault),
                candidate,
                if count < 3 {
                    DataSyncResponse::Empty
                } else {
                    DataSyncResponse::Error {
                        msg: "Vault supports at most 3 devices".into(),
                    }
                },
            )
            .await?;
        }
    }
    Ok(())
}

#[tokio::test]
async fn cold_join_admission_does_not_authorize_member_pending_or_identity_collision() -> Result<()>
{
    let (vault, candidate) = fixture(2);
    cold_response(
        Some(vault.clone()),
        vault.members()[0].user_data.clone(),
        unavailable(),
    )
    .await?;
    let pending =
        vault
            .clone()
            .update_membership(UserMembership::Outsider(UserDataOutsider::pending(
                candidate.clone(),
            )));
    cold_response(Some(pending), candidate.clone(), unavailable()).await?;
    let mut collision = candidate.clone();
    collision.device.device_name = DeviceName::generate();
    let collision_vault = vault.update_membership(UserMembership::Outsider(
        UserDataOutsider::non_member(collision),
    ));
    cold_response(Some(collision_vault), candidate, unavailable()).await?;
    Ok(())
}

#[tokio::test]
async fn cold_join_admission_missing_corrupt_or_malformed_context_fails_closed() -> Result<()> {
    let (vault, candidate) = fixture(2);
    cold_response(None, candidate.clone(), unavailable()).await?;
    let mut wrong_vault = vault.clone();
    wrong_vault.vault_name.0.push_str("-wrong");
    cold_response(Some(wrong_vault), candidate.clone(), unavailable()).await?;
    let mut corrupt = vault.clone();
    corrupt.users.insert(
        candidate.device.device_id.clone(),
        UserMembership::Member(vault.members()[0].clone()),
    );
    cold_response(Some(corrupt), candidate.clone(), unavailable()).await?;
    let mut zero_members = vault.clone();
    zero_members.users.clear();
    cold_response(Some(zero_members), candidate.clone(), unavailable()).await?;
    let (full, fourth) = fixture(3);
    let over_limit =
        full.update_membership(UserMembership::Member(UserDataMember::from(fourth.clone())));
    cold_response(Some(over_limit), fourth, unavailable()).await?;
    let mut malformed = candidate.clone();
    malformed.device.device_id = vault.members()[0].user_data.device.device_id.clone();
    cold_response(Some(vault.clone()), malformed, unavailable()).await?;
    let mut blank = candidate;
    blank.vault_name.0.clear();
    cold_response(Some(vault), blank, unavailable()).await?;
    Ok(())
}

#[tokio::test]
async fn cold_join_admission_rejects_wrong_physical_key_descriptor_and_event_kind() -> Result<()> {
    use meta_secret_core::node::common::model::vault::vault::VaultStatus;
    use meta_secret_core::node::db::descriptors::object_descriptor::ToObjectDescriptor;
    use meta_secret_core::node::db::events::{
        generic_log_event::GenericKvLogEvent, object_id::Next,
        vault::vault_status::VaultStatusObject,
    };
    for mode in 0..3 {
        let (vault, candidate) = fixture(2);
        let repo = Arc::new(InMemKvLogEventRepo::default());
        let key = KvKey::from(VaultDescriptor::from(candidate.vault_name()));
        let lookup_id = key.obj_id.clone();
        let mut object = VaultObject(KvLogEvent {
            key: key.clone(),
            value: vault,
        });
        let event = match mode {
            0 => {
                object.0.key.obj_id = lookup_id.clone().next();
                GenericKvLogEvent::Vault(object)
            }
            1 => {
                let mut other = candidate.vault_name();
                other.0.push_str("-wrong-descriptor");
                object.0.key.obj_desc = VaultDescriptor::from(other).to_obj_desc();
                GenericKvLogEvent::Vault(object)
            }
            _ => GenericKvLogEvent::VaultStatus(VaultStatusObject(KvLogEvent {
                key,
                value: VaultStatus::Outsider(UserDataOutsider::non_member(candidate.clone())),
            })),
        };
        // Corrupt the physical map deliberately; normal save derives the key from the event.
        repo.db.lock().await.insert(lookup_id, event);
        let before = repo.get_db().await;
        let master = FixtureRegistry::empty()
            .state
            .device_creds
            .server_master_key;
        let server = ServerApp::new(repo.clone(), master)?;
        let response = server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::JoinAdmission(
                candidate,
            ))))
            .await?;
        assert_eq!(response, unavailable());
        assert_eq!(
            repo.get_db().await,
            before,
            "physical corruption must never trigger repair/bootstrap/cleanup"
        );
    }
    Ok(())
}

mod read_fault {
    use super::*;
    use async_trait::async_trait;
    use meta_secret_core::node::db::{
        events::{
            generic_log_event::{GenericKvLogEvent, ToGenericEvent},
            object_id::ArtifactId,
        },
        repo::generic_db::{DbCleanUpCommand, DeleteCommand, FindOneQuery, KvLogEventRepo},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[derive(Default)]
    struct Repo {
        inner: InMemKvLogEventRepo,
        reads: AtomicUsize,
        writes: AtomicUsize,
    }
    #[async_trait(?Send)]
    impl FindOneQuery for Repo {
        async fn find_one(&self, _: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("controlled storage read failure")
        }
        async fn get_key(&self, key: ArtifactId) -> Result<Option<ArtifactId>> {
            self.find_one(key).await.map(|_| None)
        }
    }
    #[async_trait(?Send)]
    impl SaveCommand for Repo {
        async fn save<T: ToGenericEvent>(&self, _: T) -> Result<ArtifactId> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            anyhow::bail!("unexpected write")
        }
    }
    #[async_trait(?Send)]
    impl DeleteCommand for Repo {
        async fn delete(&self, _: ArtifactId) {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[async_trait(?Send)]
    impl DbCleanUpCommand for Repo {
        async fn db_clean_up(&self) {
            self.writes.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl KvLogEventRepo for Repo {}

    #[tokio::test]
    async fn cold_join_admission_storage_read_error_is_fixed_unavailable_without_writes()
    -> Result<()> {
        let (vault, candidate) = fixture(2);
        let repo = Arc::new(Repo::default());
        repo.inner
            .save(VaultObject(KvLogEvent {
                key: KvKey::from(VaultDescriptor::from(candidate.vault_name())),
                value: vault,
            }))
            .await?;
        let before = repo.inner.get_db().await;
        let server = ServerApp::new(
            repo.clone(),
            FixtureRegistry::empty()
                .state
                .device_creds
                .server_master_key,
        )?;
        let response = server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::JoinAdmission(
                candidate,
            ))))
            .await?;
        assert_eq!(response, unavailable());
        assert!(
            repo.reads.load(Ordering::SeqCst) > 0,
            "injected read failure must actually be reached"
        );
        assert_eq!(repo.writes.load(Ordering::SeqCst), 0);
        assert_eq!(repo.inner.get_db().await, before);
        Ok(())
    }
}

#[tokio::test]
async fn allowed_join_preflight_does_not_bypass_final_signed_write_capacity_check() -> Result<()> {
    use meta_secret_core::node::{
        api::WriteSyncRequest,
        db::{
            descriptors::vault_descriptor::DeviceLogDescriptor,
            events::{
                generic_log_event::ToGenericEvent,
                vault::{
                    device_log_event::DeviceLogObject,
                    vault_log_event::{
                        JoinClusterEvent, VaultActionEvent, VaultActionRequestEvent, VaultLogObject,
                    },
                },
            },
        },
        security::sign_event,
    };
    let state = FixtureRegistry::empty().state;
    let owner = UserDataMember::from(state.user_creds.client.user());
    let second = UserDataMember::from(state.user_creds.client_b.user());
    let third = UserDataMember::from(state.user_creds.vd.user());
    let candidate_creds = DeviceCredsBuilder::generate()
        .build(DeviceName::generate())
        .creds;
    let candidate = UserData {
        vault_name: owner.user_data.vault_name(),
        device: candidate_creds.device.clone(),
    };
    let two = VaultData::from(owner.clone()).update_membership(UserMembership::Member(second));
    let repo = Arc::new(InMemKvLogEventRepo::default());
    let key = KvKey::from(VaultDescriptor::from(two.vault_name.clone()));
    repo.save(VaultObject(KvLogEvent {
        key: key.clone(),
        value: two.clone(),
    }))
    .await?;
    repo.save(VaultLogObject::create(owner)).await?;
    let server = ServerApp::new(repo.clone(), state.device_creds.server_master_key)?;
    server.init().await?;
    assert_eq!(
        server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::JoinAdmission(
                candidate.clone()
            ))))
            .await?,
        DataSyncResponse::Empty
    );
    // Deterministically model a different request filling the final slot between the two requests.
    let full = two.update_membership(UserMembership::Member(third));
    repo.replace(VaultObject(KvLogEvent { key, value: full }))
        .await?;
    let before = repo.get_db().await;
    let event = DeviceLogObject(KvLogEvent {
        key: KvKey::from(DeviceLogDescriptor::from(candidate.user_id())),
        value: VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(
            JoinClusterEvent::from(candidate.clone()),
        )),
    });
    let keys = candidate_creds.key_manager()?;
    let signed = sign_event(event.to_generic(), candidate.device.device_id, &keys.dsa)?;
    let result = server
        .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
            signed,
        ))))
        .await;
    let error = result.expect_err("earlier Allowed is advisory, never a reservation or approval");
    assert!(format!("{error:#}").contains("Vault supports at most 3 devices"));
    assert_eq!(
        repo.get_db().await,
        before,
        "final signed write must leave the now-full Vault and logs unchanged"
    );
    Ok(())
}
