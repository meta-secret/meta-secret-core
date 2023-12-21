use std::sync::Arc;

use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use crate::CoreResult;
use crate::crypto::keys::KeyManager;
use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::{DeviceLink, DeviceLinkBuilder};
use crate::node::common::model::secret::{MetaPasswordId, SecretDistributionData, SecretDistributionType};
use crate::node::common::model::user::UserCredentials;
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::shared_secret::SharedSecretDescriptor;
use crate::node::db::events::common::SharedSecretObject;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;

pub mod data_block;
pub mod shared_secret;

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
    user: Arc<UserCredentials>,
    vault: VaultData,
}

impl MetaEncryptor {
    /// Algorithm:
    ///  - generate meta password id
    ///  - split
    ///  - encrypt each share with ECIES Encryption Scheme
    fn encrypt(self, password: String) -> anyhow::Result<Vec<EncryptedMessage>> {
        let cfg = SharedSecretConfig::default();

        let key_manager = KeyManager::try_from(&self.user.device_creds.secret_box)?;

        let shares = split(password, cfg).unwrap();

        let mut encrypted_shares = vec![];

        for (index, receiver) in self.vault.members().iter().enumerate() {
            let share: &UserShareDto = &shares[index];

            let share_str = serde_json::to_string(&share).unwrap();

            let receiver_pk = receiver.clone().user().device.keys.transport_pk.clone();

            let encrypted_share = key_manager
                .transport_key_pair
                .encrypt_string(share_str, receiver_pk)?;

            let device_link = DeviceLinkBuilder::new()
                .sender(self.user.device_creds.device.id.clone())
                .receiver(receiver.clone().user().device.id.clone())
                .build()?;

            let cipher_share = EncryptedMessage::CipherShare { device_link, share: encrypted_share };
            encrypted_shares.push(cipher_share);
        }

        Ok(encrypted_shares)
    }
}

pub struct MetaDistributor<Repo: KvLogEventRepo> {
    pub persistent_obj: Arc<PersistentObject<Repo>>,
    pub user_creds: Arc<UserCredentials>,
    pub vault: VaultData,
}

impl<Repo: KvLogEventRepo> MetaDistributor<Repo> {
    pub async fn distribute(self, password_id: String, password: String) -> anyhow::Result<()> {
        let encryptor = MetaEncryptor {
            user: self.user_creds.clone(),
            vault: self.vault.clone(),
        };

        let pass_id = MetaPasswordId::generate(password_id);

        //save meta password!!!
        let vault_name = self.user_creds.vault_name.clone();

        let encrypted_shares = encryptor.encrypt(password)?;

        for secret_share in encrypted_shares {
            let distribution_share = SecretDistributionData {
                distribution_type: SecretDistributionType::Split,
                vault_name: vault_name.clone(),
                secret_message: secret_share.clone(),
                meta_password_id: pass_id.clone(),
            };

            let ss_obj = match secret_share.device_link() {
                DeviceLink::Loopback(_) => {
                    let ss_local_desc = SharedSecretDescriptor::LocalShare {
                        vault_name: vault_name.clone(),
                        meta_pass_id: pass_id.clone(),
                    };

                    SharedSecretObject::LocalShare(KvLogEvent {
                        key: KvKey::unit(ObjectDescriptor::SharedSecret(ss_local_desc)),
                        value: distribution_share,
                    })
                }
                DeviceLink::PeerToPeer { .. } => {
                    let split_key = {
                        let split_obj_desc = ObjectDescriptor::from(&distribution_share);
                        KvKey::unit(split_obj_desc)
                    };

                    SharedSecretObject::Split(KvLogEvent {
                        key: split_key.clone(),
                        value: distribution_share,
                    })
                }
            };

            let _ = self
                .persistent_obj
                .repo
                .save(ss_obj.to_generic())
                .await;
        }

        Ok(())
    }
}
