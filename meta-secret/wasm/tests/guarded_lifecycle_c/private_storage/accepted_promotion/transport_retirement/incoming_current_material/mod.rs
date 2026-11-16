//! First actual uncut SS import: retired prospective K1 does not own different K2.
mod crypto;
mod lifecycle;
mod proof;
mod repo;
mod wire;
use super::*;
use async_trait::async_trait;
use meta_secret_core::{
    crypto::utils::sha256_hex,
    node::{
        app::meta_app::{
            client_transfer::MetaClientRequestTransfer,
            meta_client_service::{
                MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
            },
        },
        common::model::{
            ApplicationState, VaultFullInfo,
            secret::{SsClaim, SsClaimId, SsDistributionId},
            user::user_creds::UserCreds,
        },
        db::{
            descriptors::shared_secret_descriptor::SsWorkflowDescriptor,
            objects::persistent_object::PersistentObject, repo::generic_db::*,
        },
        security::canonical_json,
    },
    recover_from_shares,
    secret::shared_secret::UserShareDto,
};
use std::collections::HashMap;
type Inner = ExistingRepo<super::super::repo::Observed<local_repo::Repo>>;
fn hash<T: serde::Serialize>(x: &T) -> Result<String> {
    Ok(sha256_hex(&String::from_utf8(canonical_json(x)?)?))
}
fn claim_identity(c: &SsClaim) -> Result<String> {
    hash(&(
        &c.id,
        &c.dist_claim_id,
        &c.vault_name,
        &c.sender,
        &c.distribution_type,
        &c.receivers,
    ))
}
fn witness(e: GenericKvLogEvent) -> Result<LocalGuardedPreparationObject> {
    e.try_into()
}
