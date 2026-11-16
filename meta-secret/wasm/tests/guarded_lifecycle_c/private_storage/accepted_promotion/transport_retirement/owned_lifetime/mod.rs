//! Real published-own selectors, later Join lifetime, and nested compact codec delta.
mod actor;
mod codec;
mod continuation;
mod crypto;
mod harness;
mod peers;
mod privacy;
mod proof;
mod repo;
mod selectors;
mod singleton;
mod wire;

use super::super::proof as publication;
use super::*;
use async_trait::async_trait;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        app::{
            meta_app::{
                client_transfer::MetaClientRequestTransfer,
                meta_client_service::{
                    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
                },
            },
            orchestrator::MetaOrchestrator,
        },
        common::model::{
            ApplicationState, VaultFullInfo,
            secret::{SecretDistributionType, SsClaim, SsDistributionId, SsDistributionStatus},
        },
        db::{
            actions::sign_up::join::JoinActionUpdate,
            descriptors::shared_secret_descriptor::{SsLogDescriptor, SsWorkflowDescriptor},
            events::local_guarded_preparation::GuardedCompactAttempt,
            objects::persistent_object::PersistentObject,
            repo::generic_db::*,
        },
        security::canonical_json,
    },
    recover_from_shares,
    secret::shared_secret::UserShareDto,
};

type Inner = ExistingRepo<super::super::repo::Observed<local_repo::Repo>>;
fn digest<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(value)?)?))
}
fn packet_id(pass: &MetaPasswordId, who: &UserCreds) -> ArtifactId {
    ArtifactId::from(SsWorkflowDescriptor::Distribution(SsDistributionId {
        pass_id: pass.clone(),
        receiver: who.device_id().clone(),
    }))
}
fn compact(w: &LocalGuardedPreparationObject) -> Result<GuardedCompactAttempt> {
    let x = &w.0.value;
    Ok(GuardedCompactAttempt {
        origin: x.origin.clone().context("actual compact origin")?,
        transport_noop: x.transport_noop.clone(),
        phase: x.phase,
        base_vault: x.base_vault.clone(),
        requester_id: x.requester_id.clone(),
        target_id: x.target_id.clone(),
        survivors: x.survivors.clone(),
        claims: x.claims.clone(),
        packets: x.packets.clone(),
    })
}

use std::collections::HashMap;
