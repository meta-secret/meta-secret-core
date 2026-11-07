//! Actual mobile ApplicationManager / Core service / temporary SQLite / signed HTTP server.
use super::{OWNER, harness::Harness};
use crate::app_manager::ApplicationManager;
use anyhow::Result;
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        app::{
            app_manager_shared::build_client_components,
            sync::{api_url::ApiUrl, sync_protocol::HttpSyncProtocol},
        },
        common::model::{
            ApplicationState, VaultFullInfo,
            device::common::{DeviceName, DeviceType},
            user::common::UserDataOutsiderStatus,
        },
        db::{
            objects::{
                persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
                persistent_vault::PersistentVault,
            },
            repo::persistent_credentials::PersistentCredentials,
        },
    },
};
use std::{sync::Arc, thread::JoinHandle};

struct ServiceOwner {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for ServiceOwner {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(thread) = self.thread.take() {
            assert!(thread.join().is_ok());
        }
    }
}

#[test]
fn fourth_mobile_signup_rejection_keeps_get_state_usable_without_join_outbox() -> Result<()> {
    registration_attempt(3)
}

#[test]
fn third_mobile_signup_queues_one_join_and_waits_for_member_approval() -> Result<()> {
    registration_attempt(2)
}

fn registration_attempt(members: usize) -> Result<()> {
    let _owner = OWNER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let existing = Harness::new_registration_capacity(members)?;
    existing.healthy_state()?;
    let dir = tempfile::tempdir()?;
    let url = format!(
        "file:{}",
        dir.path().join("fourth-registration.sqlite").display()
    );
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let repo = Arc::new(SqlIteRepo { conn_url: url });
    let master = FixtureRegistry::empty()
        .state
        .device_creds
        .client_master_key;
    let protocol = Arc::new(HttpSyncProtocol {
        api_url: ApiUrl::custom_dev("http://127.0.0.1", u32::from(existing.http.port)),
    });
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (gateway, service) = rt.block_on(build_client_components(
        repo,
        protocol.clone(),
        master.clone(),
        DeviceName::generate(),
        DeviceType::other(),
    ))?;
    let manager = ApplicationManager::new(protocol, gateway, service.clone(), master.clone());
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let running = service.clone();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            tokio::select! { _ = Box::pin(running.run()) => {}, _ = stopped => {} }
        });
    });
    let _service_owner = ServiceOwner {
        stop: Some(stop),
        thread: Some(thread),
    };
    rt.block_on(async {
        let canonical = PersistentVault::from(existing.p.clone());
        let original = canonical.get_vault(existing.actor.vault_name.clone()).await?.to_data();
        assert_eq!(original.members().len(), members);
        let prepared = manager.generate_user_creds(existing.actor.vault_name.clone()).await?;
        assert!(matches!(prepared, ApplicationState::Vault(VaultFullInfo::Outsider(ref outsider)) if outsider.status == UserDataOutsiderStatus::NonMember));
        let candidate = PersistentCredentials { p_obj: service.p_obj.clone(), master_key: master }.get_user_creds().await?.unwrap();
        assert!(!original.is_member(candidate.device_id()));
        let log = PersistentDeviceLog::from(service.p_obj.clone());
        let before = log.find_tail_id(&candidate.user_id()).await?;
        let result = manager.sign_up().await;
        if members == 3 {
            let error = result.expect_err("fourth actual mobile Join must be rejected");
            assert_eq!(log.find_tail_id(&candidate.user_id()).await?, before, "rejected fourth Join must not poison the device outbox");
            assert!(log.latest_join_request(&candidate.user()).await?.is_none());
            assert!(format!("{error:#}").contains("Vault supports at most 3 devices"), "actual capacity rejection required");
        } else {
            let pending = result.expect("under-capacity actual mobile Join must succeed");
            assert!(matches!(pending, ApplicationState::Vault(VaultFullInfo::Outsider(ref outsider)) if outsider.user_data == candidate.user() && outsider.status == UserDataOutsiderStatus::Pending));
            let request = log.latest_join_request(&candidate.user()).await?.expect("one actual Join queued");
            assert_eq!(request.candidate, candidate.user());
            use meta_secret_core::node::db::{descriptors::vault_descriptor::DeviceLogDescriptor, events::vault::{device_log_event::DeviceLogObject, vault_log_event::{VaultActionEvent, VaultActionRequestEvent}}};
            let events: Vec<DeviceLogObject> = service.p_obj.get_object_events_from_beginning(DeviceLogDescriptor::from(candidate.user_id())).await?;
            assert_eq!(events.iter().filter(|event| matches!(&event.0.value, VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(join)) if join.candidate == candidate.user())).count(), 1, "exactly one Join, no automatic retry");
        }
        let state = manager.get_state().await?;
        assert!(matches!(state, ApplicationState::Vault(VaultFullInfo::Outsider(ref outsider)) if outsider.user_data == candidate.user() && outsider.status == if members == 3 { UserDataOutsiderStatus::NonMember } else { UserDataOutsiderStatus::Pending }));
        assert_eq!(canonical.get_vault(existing.actor.vault_name.clone()).await?.to_data(), original);
        let server = PersistentVault::from(Arc::new(PersistentObject::new(existing.server_repo.clone())));
        let server_vault = server.get_vault(existing.actor.vault_name.clone()).await?.to_data();
        if members == 3 {
            assert_eq!(server_vault, original, "rejected Join leaves server Vault unchanged");
        } else {
            assert_eq!(server_vault.members().len(), members);
            assert!(original.members().into_iter().all(|member| server_vault.is_member(&member.user_data.device.device_id)));
            assert!(!server_vault.is_member(candidate.device_id()), "Join is Pending, never implicit approval");
            assert_eq!(server_vault.secrets, original.secrets);
        }
        Ok::<_, anyhow::Error>(())
    })?;
    existing.healthy_state()?;
    Ok(())
}
