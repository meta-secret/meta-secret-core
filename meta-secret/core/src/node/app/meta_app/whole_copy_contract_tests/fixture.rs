use super::super::liveboard_material::Material;
use crate::{
    meta_tests::fixture_util::fixture::FixtureRegistry,
    node::{
        common::model::{
            crypto::aead::EncryptedMessage,
            meta_pass::{MetaPasswordId, PlainPassInfo, SecurePassInfo},
            secret::*,
            user::{common::*, user_creds::UserCreds},
            vault::{vault::*, vault_data::VaultData},
        },
        db::{
            descriptors::{shared_secret_descriptor::*, vault_descriptor::VaultDescriptor},
            events::{
                kv_log_event::*, shared_secret_event::SsWorkflowObject,
                vault::vault_event::VaultObject,
            },
            in_mem_db::InMemKvLogEventRepo,
            objects::{
                persistent_object::PersistentObject,
                persistent_shared_secret::PersistentSharedSecret,
            },
            repo::generic_db::*,
        },
    },
    secret::{
        MetaDistributor,
        shared_secret::{PlainText, UserShareDto},
    },
};
use anyhow::{Context, Result};
use std::sync::Arc;
pub const VALUE: &str = "12345678901234567890123456789012345678901234567890123456789012345";
pub struct Fixture {
    pub p: Arc<PersistentObject<InMemKvLogEventRepo>>,
    pub creds: UserCreds,
    pub peer: UserCreds,
    pub member: VaultMember,
    pub pass: MetaPasswordId,
    pub claims: SsLogData,
}
impl Fixture {
    pub async fn new(stored_n: usize, current_n: usize) -> Result<Self> {
        let state = FixtureRegistry::empty().state;
        let creds = state.user_creds.client;
        let peer = state.user_creds.client_b;
        let third = state.user_creds.vd;
        let build = |count: usize| {
            let mut v = VaultData::from(UserDataMember::from(creds.user()));
            for user in [&peer, &third].into_iter().take(count - 1) {
                v = v.update_membership(UserMembership::Member(user.user().into()));
            }
            v
        };
        let input = PlainPassInfo::new("whole-copy-unit".into(), VALUE.into());
        let pass = input.pass_id.clone();
        let stored = build(stored_n).add_secret(pass.clone());
        let p = Arc::new(PersistentObject::new(Arc::new(
            InMemKvLogEventRepo::default(),
        )));
        p.repo
            .save(VaultObject(KvLogEvent {
                key: KvKey::from(VaultDescriptor::from(creds.vault_name.clone())),
                value: stored.clone(),
            }))
            .await?;
        let vm = VaultMember {
            member: creds.user().into(),
            vault: stored,
        };
        MetaDistributor {
            p_obj: p.clone(),
            user_creds: Arc::new(creds.clone()),
            vault_member: vm.clone(),
        }
        .distribute(vm, SecurePassInfo::from(input))
        .await?;
        let ss = PersistentSharedSecret::from(p.clone());
        let claim = ss
            .find_ss_device_log_tail_event(creds.device_id().clone())
            .await?
            .context("actual encrypted unit split")?
            .to_distribution_request();
        let mut claims = SsLogData::default();
        claims.claims.insert(claim.id.clone(), claim);
        let current = build(current_n).add_secret(pass.clone());
        Ok(Self {
            p,
            creds: creds.clone(),
            peer,
            member: VaultMember {
                member: creds.user().into(),
                vault: current,
            },
            pass,
            claims,
        })
    }
    pub fn material(&self) -> Material<'_, InMemKvLogEventRepo> {
        Material {
            p_obj: self.p.clone(),
            creds: &self.creds,
            member: &self.member,
            claims: &self.claims,
        }
    }
    pub async fn event(&self) -> Result<SsWorkflowObject> {
        self.p
            .find_tail_event(SsWorkflowDescriptor::Distribution(SsDistributionId {
                pass_id: self.pass.clone(),
                receiver: self.creds.device_id().clone(),
            }))
            .await?
            .context("actual exact unit distribution")
    }
    pub async fn replace(&self, event: SsWorkflowObject) -> Result<()> {
        self.p.repo.replace(event).await?;
        Ok(())
    }
    pub async fn edit_share(&self, edit: impl FnOnce(&mut UserShareDto)) -> Result<()> {
        let SsWorkflowObject::Distribution(mut event) = self.event().await? else {
            anyhow::bail!("distribution fixture");
        };
        let data = &mut event.value;
        let clear = data
            .secret_message
            .cipher_text()
            .decrypt(&self.creds.device_creds.secret_box.transport.sk)?;
        let mut share = UserShareDto::try_from(&clear.msg)?;
        edit(&mut share);
        data.secret_message = EncryptedMessage::CipherShare {
            share: self
                .creds
                .device_creds
                .key_manager()?
                .transport
                .encrypt_string(
                    PlainText::from(share.as_json()?),
                    &self.creds.device().keys.transport_pk,
                )?,
        };
        self.replace(SsWorkflowObject::Distribution(event)).await
    }
    pub async fn denied(&self) -> Result<()> {
        let before = self.p.repo.get_db().await;
        let result = self.material().local(&self.pass).await;
        let denied = !matches!(result, Ok(Some(_)));
        assert!(
            before == self.p.repo.get_db().await,
            "availability must not mutate invalid records"
        );
        assert!(denied, "invalid exact material must fail closed");
        Ok(())
    }
}
