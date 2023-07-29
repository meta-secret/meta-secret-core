use crate::crypto::keys::KeyManager;
use crate::models::{
    AeadCipherText, EncryptedMessage, MetaPasswordDoc, MetaPasswordId, MetaPasswordRequest, SecretDistributionDocData,
    SecretDistributionType, UserCredentials, UserSecurityBox, UserSignature, VaultDoc,
};
use crate::node::db::commit_log::MetaDbManager;
use crate::node::db::generic_db::KvLogEventRepo;

use crate::node::db::events::object_id::IdGen;
use crate::node::db::models::{GenericKvLogEvent, KvKey, KvLogEvent, MetaPassObject, ObjectDescriptor};
use crate::node::server::data_sync::MetaLogger;
use crate::CoreResult;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};

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
    security_box: UserSecurityBox,
    vault: VaultDoc,
}

impl MetaEncryptor {
    /// Algorithm:
    ///  - generate meta password id
    ///  - split
    ///  - encrypt each share with ECIES Encryption Scheme
    fn encrypt(self, password: String) -> Vec<MetaCipherShare> {
        let cfg = SharedSecretConfig::default();

        let key_manager = KeyManager::try_from(self.security_box.key_manager.as_ref()).unwrap();

        let shares = split(password, cfg).unwrap();

        let mut encrypted_shares = vec![];

        for index in 0..self.vault.signatures.len() {
            let receiver_sig = &self.vault.signatures[index];
            let share: &UserShareDto = &shares[index];

            let share_str = serde_json::to_string(&share).unwrap();
            let receiver_pk = receiver_sig.public_key.as_ref().clone();

            let encrypted_share: AeadCipherText = key_manager
                .transport_key_pair
                .encrypt_string(share_str, receiver_pk)
                .unwrap();

            encrypted_shares.push(MetaCipherShare {
                receiver: receiver_sig.clone(),
                cipher_share: encrypted_share,
            });
        }

        encrypted_shares
    }
}

struct MetaCipherShare {
    receiver: UserSignature,
    cipher_share: AeadCipherText,
}

pub struct MetaDistributor<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: std::error::Error> {
    pub meta_db_manager: MetaDbManager<Repo, L, Err>,
    pub user_creds: UserCredentials,
    pub vault: VaultDoc,
}

impl<Repo: KvLogEventRepo<Err>, L: MetaLogger, Err: std::error::Error> MetaDistributor<Repo, L, Err> {
    /// Encrypt and distribute password across the cluster
    pub async fn distribute(self, password_id: String, password: String) {
        let encryptor = MetaEncryptor {
            security_box: self.user_creds.security_box.as_ref().clone(),
            vault: self.vault.clone(),
        };

        let pass = {
            let pass_id = Box::new(MetaPasswordId::generate(password_id));

            MetaPasswordDoc {
                id: pass_id,
                vault: Box::new(self.vault.clone()),
            }
        };

        //save meta password!!!
        let vault_name = self.user_creds.user_sig.vault.name.clone();
        let meta_pass_obj_desc = ObjectDescriptor::MetaPassword { vault_name };

        let pass_tail_id = self
            .meta_db_manager
            .persistent_obj
            .find_tail_id_by_obj_desc(&meta_pass_obj_desc)
            .await
            .map(|id| id.next())
            .unwrap();

        let meta_pass_event = GenericKvLogEvent::MetaPass(MetaPassObject::Update {
            event: KvLogEvent {
                key: KvKey {
                    obj_id: pass_tail_id,
                    obj_desc: meta_pass_obj_desc,
                },
                value: pass.clone(),
            },
        });

        self.meta_db_manager.repo.save_event(&meta_pass_event).await.unwrap();

        let encrypted_shares = encryptor.encrypt(password);
        for cipher_share in encrypted_shares {
            let cipher_msg = EncryptedMessage {
                receiver: Box::from(cipher_share.receiver.clone()),
                encrypted_text: Box::new(cipher_share.cipher_share),
            };

            let distribution_share = SecretDistributionDocData {
                distribution_type: SecretDistributionType::Split,
                meta_password: Box::new(MetaPasswordRequest {
                    user_sig: Box::new(self.user_creds.user_sig.as_ref().clone()),
                    meta_password: Box::new(pass.clone()),
                }),
                secret_message: Box::new(cipher_msg),
            };

            //server_api::distribute(&distribution_share).await.unwrap();
        }
    }
}
