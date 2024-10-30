use std::sync::Arc;

use crate::crypto::keys::KeyManager;
use crate::node::common::model::crypto::EncryptedMessage;
use crate::node::common::model::device::device_link::{DeviceLink, DeviceLinkBuilder, PeerToPeerDeviceLink};
use crate::node::common::model::secret::{MetaPasswordId, SsDistributionClaimId, SsDistributionId, SecretDistributionData, SecretDistributionType, SsDistributionClaim};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::{VaultData, VaultMember, VaultStatus};
use crate::node::db::descriptors::object_descriptor::{ObjectDescriptor, ToObjectDescriptor};
use crate::node::db::descriptors::shared_secret_descriptor::SharedSecretDescriptor;
use crate::node::db::events::generic_log_event::ToGenericEvent;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::SharedSecretObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::CoreResult;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;

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
    fn encrypt(
        self, password: String, cfg: SharedSecretConfig
    ) -> anyhow::Result<Vec<EncryptedMessage>> {
        let key_manager = KeyManager::try_from(&self.user.device_creds.secret_box)?;

        let shares = split(password, cfg)?;

        let mut encrypted_shares = vec![];

        for (index, receiver) in self.vault.members().iter().enumerate() {
            let share: &UserShareDto = &shares[index];

            let share_str = serde_json::to_string(&share)?;

            let receiver_pk = receiver.user().device.keys.transport_pk.clone();

            let encrypted_share = key_manager.transport.encrypt_string(share_str, receiver_pk)?;

            let device_link = DeviceLinkBuilder::builder()
                .sender(self.user.device_creds.device.device_id.clone())
                .receiver(receiver.clone().user().device.device_id.clone())
                .build()?;

            let cipher_share = EncryptedMessage::CipherShare {
                device_link,
                share: encrypted_share,
            };
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
    pub async fn distribute(self, password_id: String, password: String) -> anyhow::Result<()> {
        //save meta password!!!
        let vault_name = self.user_creds.vault_name.clone();
        
        let cfg = {
            let members_num = self.vault_member.vault.members().len();
            SharedSecretConfig::calculate(members_num)
        };

        let encrypted_shares = {
            let encryptor = MetaEncryptor {
                user: self.user_creds.clone(),
                vault: self.vault_member.vault.clone(),
            };
            encryptor.encrypt(password, cfg)?
        };

        let claim_id = SsDistributionClaimId::generate();
        let distribution_type = SecretDistributionType::Split;
        let pass_id = MetaPasswordId::generate(password_id);
        
        let p_device_log = PersistentDeviceLog { p_obj: self.p_obj.clone() };
        p_device_log.save_add_meta_pass_request(self.vault_member.member, pass_id.clone()).await?;

        let distributions = encrypted_shares.iter()
            .filter_map(|share| {
                match share.device_link() {
                    DeviceLink::Loopback(_) => None,
                    DeviceLink::PeerToPeer(p2p_device_link) => Some(p2p_device_link.clone())
                }
            })
            .collect();

        let claim = SsDistributionClaim {
            vault_name: vault_name.clone(),
            id: claim_id.clone(),
            pass_id: pass_id.clone(),
            distribution_type: distribution_type.clone(),
            distributions,
        };
        
        let p_ss = PersistentSharedSecret { p_obj: self.p_obj.clone() };
        let device_id = self.user_creds.device_creds.device.device_id.clone();
        p_ss.save_claim_in_ss_device_log(device_id, claim).await?;

        for secret_share in encrypted_shares {
            let distribution_share = SecretDistributionData {
                vault_name: vault_name.clone(),
                secret_message: secret_share.clone(),
                pass_id: pass_id.clone(),
            };

            let ss_obj = match secret_share.device_link() {
                DeviceLink::Loopback(_) => {
                    let ss_local_desc = SharedSecretDescriptor::LocalShare(pass_id.clone());

                    SharedSecretObject::LocalShare(KvLogEvent {
                        key: KvKey::unit(ObjectDescriptor::SharedSecret(ss_local_desc)),
                        value: distribution_share,
                    })
                }
                DeviceLink::PeerToPeer(p2p_device_link) => {
                    //save device log event?

                    let dist_id = SsDistributionId {
                        claim_id: claim_id.clone(),
                        distribution_type,
                        device_link: p2p_device_link,
                    };

                    let split_key = {
                        let split_obj_desc = SharedSecretDescriptor::SsDistribution(dist_id)
                            .to_obj_desc();
                        KvKey::unit(split_obj_desc)
                    };

                    SharedSecretObject::SsDistribution(KvLogEvent {
                        key: split_key.clone(),
                        value: distribution_share,
                    })
                }
            };

            self.p_obj.repo.save(ss_obj.to_generic()).await?;
        }

        Ok(())
    }
}
