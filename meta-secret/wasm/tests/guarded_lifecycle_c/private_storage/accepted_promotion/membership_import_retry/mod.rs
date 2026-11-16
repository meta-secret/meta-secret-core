//! A real Join import must not consume sender resharing across error/reopen.
mod bootstrap;
mod client;
mod file_repo;
mod lifecycle;
mod proof;
mod wire;

use anyhow::{Context, Result, ensure};
use meta_db_sqlite::db::{sqlite_migration::EmbeddedMigrationsTool, sqlite_store::SqlIteRepo};
use meta_secret_core::{
    crypto::{
        key_pair::{KeyPair, TransportDsaKeyPair},
        keys::TransportSk,
    },
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        api::*,
        app::{
            orchestrator::MetaOrchestrator,
            sync::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol},
        },
        common::model::{
            device::common::DeviceId,
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
            secret::{SsClaim, SsDistributionId, SsDistributionStatus},
            user::{common::*, user_creds::UserCreds},
            vault::vault::VaultMember,
        },
        db::{
            actions::sign_up::join::JoinActionUpdate,
            descriptors::{
                shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
                vault_descriptor::DeviceLogDescriptor,
            },
            events::{
                generic_log_event::*,
                object_id::ArtifactId,
                shared_secret_event::{SsDeviceLogObject, SsWorkflowObject},
                vault::{device_log_event::DeviceLogObject, vault_log_event::*},
            },
            in_mem_db::InMemKvLogEventRepo,
            objects::{
                persistent_device_log::PersistentDeviceLog, persistent_object::PersistentObject,
                persistent_shared_secret::PersistentSharedSecret,
                persistent_vault::PersistentVault,
            },
            repo::{generic_db::*, persistent_credentials::PersistentCredentials},
        },
        security::sign_event,
    },
    secret::{MetaDistributor, shared_secret::UserShareDto},
};
use meta_server_node::server::server_app::ServerApp;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

type Remote = Arc<ServerApp<InMemKvLogEventRepo>>;
const VALUE: &str = "membership-import-real-owned-secret";

#[derive(Clone, Copy, Debug, PartialEq)]
enum FaultKind {
    DurableSaveLostReply,
    SecondVaultBefore,
}
#[derive(Clone, Copy, PartialEq)]
struct Identity {
    kind: FaultKind,
    token: usize,
}
#[derive(Debug)]
struct InjectedFault {
    kind: FaultKind,
    token: usize,
}
impl std::fmt::Display for InjectedFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("membership import test boundary")
    }
}
impl std::error::Error for InjectedFault {}
impl Identity {
    fn error(self) -> anyhow::Error {
        anyhow::Error::new(InjectedFault {
            kind: self.kind,
            token: self.token,
        })
    }
    fn original(self, result: &Result<()>) -> bool {
        result
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<InjectedFault>())
            .is_some_and(|e| e.kind == self.kind && e.token == self.token)
    }
}
fn material(event: &GenericKvLogEvent) -> bool {
    matches!(
        event,
        GenericKvLogEvent::SsWorkflow(_)
            | GenericKvLogEvent::SsDeviceLog(_)
            | GenericKvLogEvent::SsLog(_)
    )
}
