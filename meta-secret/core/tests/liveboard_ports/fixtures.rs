use super::fault_repo::Repo;
use super::transport::Protocol;
use anyhow::{Context, Result};
use meta_secret_core::crypto::keys::TransportSk;
use meta_secret_core::meta_tests::fixture_util::fixture::FixtureRegistry;
use meta_secret_core::node::app::meta_app::client_transfer::MetaClientRequestTransfer;
use meta_secret_core::node::app::meta_app::meta_client_service::{
    MetaClientDataTransfer, MetaClientService, MetaClientStateProvider,
};
use meta_secret_core::node::app::orchestrator::MetaOrchestrator;
use meta_secret_core::node::app::sync::sync_gateway::SyncGateway;
use meta_secret_core::node::common::model::meta_pass::{
    MetaPasswordId, PlainPassInfo, SecurePassInfo,
};
use meta_secret_core::node::common::model::secret::{SecretDistributionData, SsDistributionId};
use meta_secret_core::node::common::model::user::{common::*, user_creds::UserCreds};
use meta_secret_core::node::common::model::vault::{vault::*, vault_data::VaultData};
use meta_secret_core::node::db::descriptors::{
    shared_secret_descriptor::SsWorkflowDescriptor,
    vault_descriptor::{VaultDescriptor, VaultStatusDescriptor},
};
use meta_secret_core::node::db::events::{
    kv_log_event::*,
    object_id::ArtifactId,
    vault::{
        liveboard_metadata::PendingRemovalBinding, vault_event::VaultObject, vault_log_event::*,
        vault_status::VaultStatusObject,
    },
};
use meta_secret_core::node::db::objects::{
    persistent_object::PersistentObject, persistent_shared_secret::PersistentSharedSecret,
};
use meta_secret_core::node::db::repo::{
    generic_db::*, persistent_credentials::PersistentCredentials,
};
use meta_secret_core::secret::MetaDistributor;
use meta_secret_core::secret::shared_secret::UserShareDto;
use std::sync::Arc;

pub const REQUEST: &str = "e65fb169-dd44-4ecd-abd5-6f05ce7016e5";
pub const VALUE: &str = "liveboard-test-only-secret-value";

pub struct Harness {
    pub p: Arc<PersistentObject<Repo>>,
    pub initiator: UserCreds,
    pub approver: UserCreds,
    pub target: UserCreds,
    pub master: TransportSk,
    pub vault: VaultData,
    pub protocol: Arc<Protocol>,
}
impl Harness {
    pub async fn new(count: usize) -> Result<Self> {
        let state = FixtureRegistry::empty().state;
        let initiator = state.user_creds.client;
        let approver = state.user_creds.client_b;
        let target = state.user_creds.vd;
        let mut vault = VaultData::from(UserDataMember::from(approver.user()));
        if count >= 2 {
            vault = vault.update_membership(UserMembership::Member(UserDataMember::from(
                initiator.user(),
            )));
        }
        if count >= 3 {
            vault = vault
                .update_membership(UserMembership::Member(UserDataMember::from(target.user())));
        }
        let h = Self {
            p: Arc::new(PersistentObject::new(Arc::new(Repo::default()))),
            initiator,
            approver,
            target,
            master: state.device_creds.client_b_master_key,
            vault,
            protocol: Arc::new(Protocol::default()),
        };
        PersistentCredentials {
            p_obj: h.p.clone(),
            master_key: h.master.clone(),
        }
        .save_user_creds(h.approver.clone())
        .await?;
        h.save_vault().await?;
        h.p.repo
            .save(VaultLogObject::create(UserDataMember::from(
                h.approver.user(),
            )))
            .await?;
        Ok(h)
    }
    pub fn gateway(&self) -> Arc<SyncGateway<Repo, Protocol>> {
        Arc::new(SyncGateway::new(
            "liveboard-ports".into(),
            self.p.clone(),
            self.protocol.clone(),
            self.master.clone(),
        ))
    }
    pub fn reopened_gateway(&self) -> Arc<SyncGateway<Repo, Protocol>> {
        Arc::new(SyncGateway::new(
            "liveboard-ports-reopened".into(),
            Arc::new(PersistentObject::new(self.p.repo.clone())),
            self.protocol.clone(),
            self.master.clone(),
        ))
    }
    pub fn service(&self) -> MetaClientService<Repo, Protocol> {
        MetaClientService {
            data_transfer: Arc::new(MetaClientDataTransfer {
                dt: MetaClientRequestTransfer::new(),
            }),
            sync_gateway: self.gateway(),
            state_provider: Arc::new(MetaClientStateProvider::new()),
            p_obj: self.p.clone(),
            device_data: self.approver.device(),
            master_key: self.master.clone(),
        }
    }
    pub async fn save_vault(&self) -> Result<()> {
        let obj = VaultObject(KvLogEvent {
            key: KvKey::from(VaultDescriptor::from(self.vault.vault_name.clone())),
            value: self.vault.clone(),
        });
        if self
            .p
            .repo
            .find_one(obj.0.key.obj_id.clone())
            .await?
            .is_some()
        {
            self.p.repo.replace(obj).await?;
        } else {
            self.p.repo.save(obj).await?;
        }
        for member in self.vault.members() {
            let status = VaultStatusObject::new(
                VaultStatus::Member(member.clone()),
                ArtifactId::from(VaultStatusDescriptor::from(member.user_data.user_id())),
            );
            self.p.repo.save(status).await?;
        }
        Ok(())
    }
    pub async fn seed_secret(&mut self, name: &str) -> Result<MetaPasswordId> {
        let pass = PlainPassInfo::new(name.into(), VALUE.into());
        let id = pass.pass_id.clone();
        self.vault = self.vault.clone().add_secret(id.clone());
        self.save_vault().await?;
        let member = VaultMember {
            member: UserDataMember::from(self.initiator.user()),
            vault: self.vault.clone(),
        };
        // At count=1, use the sole real member rather than inventing outsider ownership.
        let owner = if self.vault.is_member(self.initiator.device_id()) {
            self.initiator.clone()
        } else {
            self.approver.clone()
        };
        let member = VaultMember {
            member: UserDataMember::from(owner.user()),
            ..member
        };
        MetaDistributor {
            p_obj: self.p.clone(),
            user_creds: Arc::new(owner.clone()),
            vault_member: member.clone(),
        }
        .distribute(member, SecurePassInfo::from(pass))
        .await?;
        let ss = PersistentSharedSecret::from(self.p.clone());
        let claim = ss
            .find_ss_device_log_tail_event(owner.device_id().clone())
            .await?
            .context("real split claim")?
            .to_distribution_request();
        ss.save_ss_log_event(claim).await?;
        Ok(id)
    }
    pub async fn seed_removal(&self) -> Result<RemoveMemberRequestEvent> {
        let request = MetaOrchestrator {
            p_obj: self.p.clone(),
            user_creds: self.initiator.clone(),
        }
        .request_remove_member(UserDataMember::from(self.target.user()))
        .await?;
        let mut log = VaultLogObject::create(UserDataMember::from(self.approver.user()));
        log.0
            .value
            .requests
            .insert(VaultActionRequestEvent::RemoveMember(request.clone()));
        log.0
            .value
            .liveboard_removal_pending
            .push(PendingRemovalBinding {
                request_id: REQUEST.into(),
                request: request.clone(),
            });
        self.p.repo.replace(log).await?;
        Ok(request)
    }
    pub async fn distribution(
        &self,
        pass: &MetaPasswordId,
        user: &UserCreds,
    ) -> Result<SecretDistributionData> {
        self.p
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: pass.clone(),
                receiver: user.device_id().clone(),
            }))
            .await?
            .context("real encrypted distribution missing")?
            .to_distribution_data()
    }
    pub async fn share(&self, pass: &MetaPasswordId, user: &UserCreds) -> Result<UserShareDto> {
        let data = self.distribution(pass, user).await?;
        let plain = data
            .secret_message
            .cipher_text()
            .decrypt(&user.device_creds.secret_box.transport.sk)?;
        Ok(UserShareDto::try_from(&plain.msg)?)
    }
}
