use std::sync::Arc;

use tracing::Instrument;

use crate::crypto::keys::{KeyManager, OpenBox, SecretBox};
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::CoreResult;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use crate::crypto::encoding::base64::Base64Text;
use crate::node::common::model::crypto::{AeadCipherText, EncryptedMessage};
use crate::node::common::model::device::{DeviceLink, DeviceLinkBuilder};
use crate::node::common::model::secret::{MetaPasswordId, SecretDistributionData, SecretDistributionType};
use crate::node::common::model::user::{UserCredentials, UserData};
use crate::node::common::model::vault::VaultData;
use crate::node::db::descriptors::object_descriptor::ObjectDescriptor;
use crate::node::db::descriptors::shared_secret::{SharedSecretDescriptor, SharedSecretEventId};
use crate::node::db::events::common::SharedSecretObject;

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

            let receiver_pk = receiver.user_data.device.keys.transport_pk.clone();

            let encrypted_share = key_manager
                .transport_key_pair
                .encrypt_string(share_str, receiver_pk)?;

            let device_link = DeviceLinkBuilder::new()
                .sender(self.user.device_creds.device.id.clone())
                .receiver(receiver.user_data.device.id.clone())
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
            user: self.user_creds,
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
                secret_message: secret_share,
                meta_password_id: pass_id.clone(),
            };

            let ss_obj = match secret_share.device_link() {
                DeviceLink::Loopback(_) => {
                    let ss_local_desc = SharedSecretDescriptor::LocalShare {
                        vault_name: vault_name.clone(),
                        meta_pass_id: pass_id.clone(),
                    };

                    SharedSecretObject::LocalShare {
                        event: KvLogEvent {
                            key: KvKey::unit(ObjectDescriptor::SharedSecret(ss_local_desc)),
                            value: distribution_share,
                        },
                    }
                }
                DeviceLink::PeerToPeer { .. } => {
                    let split_key = {
                        let split_obj_desc = ObjectDescriptor::from(&distribution_share);
                        KvKey::unit(split_obj_desc)
                    };

                    SharedSecretObject::Split {
                        event: KvLogEvent {
                            key: split_key.clone(),
                            value: distribution_share,
                        },
                    }
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

#[cfg(test)]
mod test {
    use crate::node::db::actions::join;
    use crate::node::db::events::common::PublicKeyRecord;
    use crate::node::db::events::vault_event::VaultObject;
    use crate::node::db::repo::generic_db::{FindOneQuery, SaveCommand};
    use crate::node::server::data_sync::test::DataSyncTestContext;
    use crate::node::server::data_sync::DataSyncApi;
    use crate::test_utils::meta_test_utils::MetaAppTestVerifier;
    use std::ops::Deref;
    use crate::node::db::events::object_id::ObjectId;

    use super::*;

    #[tokio::test]
    async fn test_distribute() -> anyhow::Result<()> {
        let ctx = DataSyncTestContext::default();
        let data_sync = ctx.data_sync;

        let vault_unit = GenericKvLogEvent::Vault(VaultObject::unit(&ctx.user_sig));
        data_sync.send(vault_unit).await?;

        let _user_pk = PublicKeyRecord::from(ctx.user_sig.public_key.as_ref().clone());

        let vault_unit_id = ObjectId::vault_unit("test_vault");
        let vault_tail_id = ctx.persistent_obj.find_tail_id(vault_unit_id.clone()).await.unwrap();
        let vault_event = ctx.repo.find_one(vault_tail_id).await?;

        let s_box_b = KeyManager::generate_secret_box("test_vault".to_string());
        let device_b = DeviceInfo {
            device_id: "b".to_string(),
            device_name: "b".to_string(),
        };
        let user_sig_b = s_box_b.get_user_sig(&device_b);

        let s_box_c = KeyManager::generate_secret_box("test_vault".to_string());
        let device_c = DeviceInfo {
            device_id: "c".to_string(),
            device_name: "c".to_string(),
        };
        let user_sig_c = s_box_c.get_user_sig(&device_c);

        if let Some(GenericKvLogEvent::Vault(VaultObject::SignUpUpdate { event })) = vault_event {
            let vault = event.value;

            let join_request = join::join_cluster_request(&vault_unit_id.next(), &user_sig_b);
            let kv_join_event = join::accept_join_request(&join_request, &vault);
            let accept_event = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                event: kv_join_event.clone(),
            });
            let _ = ctx.repo.save(accept_event).await;

            let join_request_c = join::join_cluster_request(&vault_unit_id.next().next(), &user_sig_c);
            let kv_join_event_c = join::accept_join_request(&join_request_c, &kv_join_event.value);
            let accept_event_c = GenericKvLogEvent::Vault(VaultObject::JoinUpdate {
                event: kv_join_event_c.clone(),
            });
            let _ = ctx.repo.save(accept_event_c).await;

            let distributor = MetaDistributor {
                persistent_obj: ctx.persistent_obj,
                user_creds: ctx.user_creds,
                vault: kv_join_event_c.value,
            };

            distributor
                .distribute(String::from("test"), String::from("t0p$ecret"))
                .await?;

            let mut db = ctx
                .repo
                .db
                .lock()
                .await
                .values()
                .cloned()
                .collect::<Vec<GenericKvLogEvent>>();
            db.sort_by(|a, b| {
                let a_id = a.key().obj_id.id_str();
                let b_id = b.key().obj_id.id_str();

                a_id.as_str().partial_cmp(b_id.as_str()).unwrap()
            });

            {
                let events = ctx.repo.as_ref().db.as_ref().lock().await.deref().clone();

                let verifier = MetaAppTestVerifier {
                    vault_name: ctx.user_sig.vault.name.clone(),
                    events,
                };
                //verifier.client_verification();
            }

            println!("total events: {}", db.len());
            for event in db {
                println!("event:");
                let id = event.key().obj_id.id_str();
                println!(" key: {}", serde_json::to_string(&id).unwrap());
                println!(" event: {}", serde_json::to_string_pretty(&event).unwrap());
            }
        } else {
            panic!("Invalid event")
        }

        Ok(())
    }
}
