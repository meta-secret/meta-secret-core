use std::sync::Arc;

use crate::node::common::model::crypto::aead::{EncryptedMessage};
use crate::node::common::model::secret::{
    SecretDistributionData, SsDistributionClaimId, SsDistributionId
};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::VaultMember;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::SharedSecretObject;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::shared_secret::UserSecretDto;
use crate::CoreResult;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use anyhow::Result;
use tracing_attributes::instrument;
use crate::node::common::model::meta_pass::MetaPasswordId;

pub mod data_block;
pub mod shared_secret;

pub fn split2(secret: String, config: SharedSecretConfig) -> CoreResult<UserSecretDto> {
    let shares = split(secret, config)?;
    Ok(UserSecretDto { shares })
}

pub fn split(secret: String, config: SharedSecretConfig) -> CoreResult<Vec<UserShareDto>> {
    let plain_text = PlainText::from(secret);
    let shared_secret = SharedSecretEncryption::new(config, &plain_text)?;

    let mut shares: Vec<UserShareDto> = vec![];
    for share_index in 0..config.number_of_shares {
        let share: UserShareDto = shared_secret.get_share(share_index);
        shares.push(share);
    }

    Ok(shares)
}

pub struct MetaEncryptor {
    creds: Arc<UserCredentials>,
    owner: VaultMember,
}

impl MetaEncryptor {
    /// Algorithm:
    ///  - generate meta password id
    ///  - split
    ///  - encrypt each share with ECIES Encryption Scheme
    fn split_and_encrypt(self, password: String) -> Result<Vec<EncryptedMessage>> {
        let secret = split2(password, self.owner.vault.sss_cfg())?;

        let mut encrypted_shares = vec![];

        for (index, receiver) in self.owner.vault.members().iter().enumerate() {
            let share = &secret.shares[index];

            let encrypted_share = {
                let share_str = PlainText::from(share.as_json()?);
                let receiver_pk = &receiver.user().device.keys.transport_pk();
                self.creds
                    .device_creds
                    .key_manager()?
                    .transport
                    .encrypt_string(share_str, receiver_pk)?
            };

            let cipher_share = EncryptedMessage::CipherShare { share: encrypted_share };
            encrypted_shares.push(cipher_share);
        }

        Ok(encrypted_shares)
    }
}

pub struct MetaDistributor<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub user_creds: Arc<UserCredentials>,
    pub vault_member: VaultMember,
}

impl<Repo: KvLogEventRepo> MetaDistributor<Repo> {
    #[instrument(skip(self, password))]
    pub async fn distribute(
        self,
        vault_member: VaultMember,
        password_id: MetaPasswordId,
        password: String,
    ) -> Result<()> {
        //save meta password!!!
        let vault_name = self.user_creds.vault_name.clone();

        let encrypted_shares = {
            let encryptor = MetaEncryptor {
                creds: self.user_creds.clone(),
                owner: self.vault_member.clone(),
            };
            encryptor.split_and_encrypt(password)?
        };

        let claim = vault_member.create_split_claim(password_id);

        let p_device_log = PersistentDeviceLog {
            p_obj: self.p_obj.clone(),
        };
        p_device_log
            .save_add_meta_pass_request(self.vault_member.member, claim.id.pass_id.clone())
            .await?;

        let p_ss = PersistentSharedSecret {
            p_obj: self.p_obj.clone(),
        };
        p_ss.save_claim_in_ss_device_log(claim.clone()).await?;

        for secret_share in encrypted_shares {
            let distribution_share = SecretDistributionData {
                vault_name: vault_name.clone(),
                claim_id: SsDistributionClaimId::from(claim.id.pass_id.clone()),
                secret_message: secret_share.clone()
            };

            let dist_id = {
                let receiver = secret_share.cipher_text().auth_data
                    .channel()
                    .receiver()
                    .to_device_id();
                SsDistributionId { pass_id: claim.id.pass_id.clone(), receiver }
            };

            let split_key = {
                let split_obj_desc = SharedSecretDescriptor::SsDistribution(dist_id).to_obj_desc();
                KvKey::unit(split_obj_desc)
            };

            let ss_obj = SharedSecretObject::SsDistribution(KvLogEvent {
                key: split_key.clone(),
                value: distribution_share,
            });

            self.p_obj.repo.save(ss_obj.to_generic()).await?;
        }

        Ok(())
    }
}
