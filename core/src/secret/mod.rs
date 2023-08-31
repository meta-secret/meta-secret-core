use crate::crypto::keys::KeyManager;
use crate::models::{
    AeadCipherText, EncryptedMessage, MetaPasswordDoc, MetaPasswordId, MetaPasswordRequest, SecretDistributionDocData,
    SecretDistributionType, UserCredentials, UserSecurityBox, UserSignature, VaultDoc,
};
use crate::node::db::events::common::ObjectCreator;
use crate::node::db::events::common::{MetaPassObject, SharedSecretObject};
use crate::node::db::events::generic_log_event::GenericKvLogEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::local::KvLogEventLocal;
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};
use crate::node::db::meta_db::meta_db_manager::MetaDbManager;
use crate::node::logger::MetaLogger;
use crate::CoreResult;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use crate::node::db::generic_db::KvLogEventRepo;

pub mod data_block;
pub mod shared_secret;
use std::rc::Rc;

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

pub struct MetaDistributor<Repo: KvLogEventRepo, Logger: MetaLogger> {
    pub meta_db_manager: Rc<MetaDbManager<Repo, Logger>>,
    pub user_creds: Rc<UserCredentials>,
    pub vault: VaultDoc,
}

impl<Repo: KvLogEventRepo, Logger: MetaLogger> MetaDistributor<Repo, Logger> {
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
                key: KvKey::Key {
                    obj_id: pass_tail_id,
                    obj_desc: meta_pass_obj_desc,
                },
                value: pass.clone(),
            },
        });

        self.meta_db_manager.repo.save_event(&meta_pass_event).await.unwrap();

        let encrypted_shares = encryptor.encrypt(password);

        for cipher_share in encrypted_shares.iter() {
            let cipher_msg = EncryptedMessage {
                receiver: Box::from(cipher_share.receiver.clone()),
                encrypted_text: Box::new(cipher_share.cipher_share.clone()),
            };

            let distribution_share = SecretDistributionDocData {
                distribution_type: SecretDistributionType::Split,
                meta_password: Box::new(MetaPasswordRequest {
                    user_sig: Box::new(self.user_creds.user_sig.as_ref().clone()),
                    meta_password: Box::new(pass.clone()),
                }),
                secret_message: Box::new(cipher_msg),
            };

            let meta_pass_id = pass.id.id.clone();

            if cipher_share.receiver.vault.device.device_id == self.user_creds.user_sig.vault.device.device_id {
                let secret_share_event = GenericKvLogEvent::LocalEvent(KvLogEventLocal::LocalSecretShare {
                    event: KvLogEvent {
                        key: KvKey::unit(&ObjectDescriptor::LocalSecretShare { meta_pass_id }),
                        value: distribution_share,
                    },
                });

                let _ = self.meta_db_manager
                    .repo
                    .save_event(&secret_share_event)
                    .await;

            } else {
                let obj_desc = ObjectDescriptor::from(&distribution_share);
                let secret_share_event = GenericKvLogEvent::SharedSecret(SharedSecretObject::Split {
                    event: KvLogEvent {
                        key: KvKey::Empty { obj_desc: obj_desc.clone() },
                        value: distribution_share,
                    },
                });

                let tail_id = self
                    .meta_db_manager
                    .persistent_obj
                    .find_tail_id_by_obj_desc(&obj_desc)
                    .await
                    .map(|id| id.next())
                    .unwrap_or(ObjectId::unit(&obj_desc));

                let _ = self.meta_db_manager
                    .repo
                    .save(&tail_id, &secret_share_event)
                    .await;
            };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::node::server::data_sync::test::DataSyncTestContext;
    use crate::node::db::events::vault_event::VaultObject;
    use crate::node::server::data_sync::DataSyncApi;
    use crate::node::db::events::common::{LogEventKeyBasedRecord, PublicKeyRecord};
    use crate::node::db::actions::join;
    use crate::models::DeviceInfo;
    use crate::node::db::generic_db::{FindOneQuery, SaveCommand};

    #[tokio::test]
    async fn test() {
        let ctx = DataSyncTestContext::new();
        let data_sync = ctx.data_sync;

        let vault_unit = GenericKvLogEvent::Vault(VaultObject::unit(&ctx.user_sig));
        data_sync.send(&vault_unit).await;

        let _user_pk = PublicKeyRecord::from(ctx.user_sig.public_key.as_ref().clone());

        let vault_unit_id = ObjectId::vault_unit("test_vault");
        let vault_tail_id = ctx.persistent_obj.find_tail_id(&vault_unit_id).await.unwrap();
        let vault_event = ctx.repo.find_one(&vault_tail_id).await.unwrap().unwrap();

        let s_box_b = KeyManager::generate_security_box("test_vault".to_string());
        let device_b = DeviceInfo {
            device_id: "b".to_string(),
            device_name: "b".to_string(),
        };
        let user_sig_b = s_box_b.get_user_sig(&device_b);

        let s_box_c = KeyManager::generate_security_box("test_vault".to_string());
        let device_c = DeviceInfo {
            device_id: "c".to_string(),
            device_name: "c".to_string(),
        };
        let user_sig_c = s_box_c.get_user_sig(&device_c);

        if let GenericKvLogEvent::Vault(VaultObject::SignUpUpdate { event }) = vault_event {
            let vault = event.value;

            let join_request = join::join_cluster_request(&vault_unit_id.next().next(), &user_sig_b);
            let kv_join_event = join::accept_join_request(&join_request, &vault);
            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                event: kv_join_event.clone(),
            });
            let _ = ctx.meta_db_manager
                .persistent_obj
                .repo
                .save_event(&accept_event)
                .await;

            let join_request_c = join::join_cluster_request(&vault_unit_id.next().next().next(), &user_sig_c);
            let kv_join_event_c = join::accept_join_request(&join_request_c, &kv_join_event.value);
            let accept_event_c = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                event: kv_join_event_c.clone(),
            });
            let _ = ctx.meta_db_manager
                .persistent_obj
                .repo
                .save_event(&accept_event_c)
                .await;

            let distributor = MetaDistributor {
                meta_db_manager: ctx.meta_db_manager,
                user_creds: ctx.user_creds,
                vault: kv_join_event_c.value,
            };

            distributor.distribute(String::from("test"), String::from("t0p$ecret")).await;

            let mut db = ctx.repo.db.take().values().cloned().collect::<Vec<GenericKvLogEvent>>();
            db.sort_by(|a, b| {
                let a_id = match a.key() {
                    KvKey::Empty { obj_desc } => {
                        obj_desc.to_id()
                    }
                    KvKey::Key { obj_id, .. } => {
                        obj_id.id_str()
                    }
                };

                let b_id = match b.key() {
                    KvKey::Empty { obj_desc } => {
                        obj_desc.to_id()
                    }
                    KvKey::Key { obj_id, .. } => {
                        obj_id.id_str()
                    }
                };

                a_id.as_str().partial_cmp(b_id.as_str()).unwrap()
            });

            println!("total events: {}", db.len());
            for event in db {
                println!("event:");
                let id = match event.key() {
                    KvKey::Empty { obj_desc } => {
                        obj_desc.to_id()
                    }
                    KvKey::Key { obj_id, .. } => {
                        obj_id.id_str()
                    }
                };
                println!(" key: {}", serde_json::to_string(&id).unwrap());
                println!(" event: {}", serde_json::to_string(&event).unwrap());
            }
        } else {
            panic!("Invalid event")
        }
    }
}