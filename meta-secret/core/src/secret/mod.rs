use std::sync::Arc;

use crate::CoreResult;
use crate::node::common::model::crypto::aead::EncryptedMessage;
use crate::node::common::model::meta_pass::{SecurePassInfo};
use crate::node::common::model::secret::{SecretDistributionData, SsDistributionId};
use crate::node::common::model::user::user_creds::UserCredentials;
use crate::node::common::model::vault::vault::VaultMember;
use crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::shared_secret_event::SsWorkflowObject;
use crate::node::db::events::vault::vault_log_event::AddMetaPassEvent;
use crate::node::db::objects::persistent_device_log::PersistentDeviceLog;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use crate::secret::shared_secret::UserSecretDto;
use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use anyhow::Result;
use tracing_attributes::instrument;
use secrecy::SecretString;

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
    fn split_and_encrypt(self, password: SecretString) -> Result<Vec<EncryptedMessage>> {
        // Safely get the password string
        let password_str = secrecy::ExposeSecret::expose_secret(&password).to_string();
        let secret = split2(password_str, self.owner.vault.sss_cfg())?;

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

            let cipher_share = EncryptedMessage::CipherShare {
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

/// Save meta password!!!
impl<Repo: KvLogEventRepo> MetaDistributor<Repo> {
    #[instrument(skip(self, pass_info))]
    pub async fn distribute(self, vault_member: VaultMember, pass_info: SecurePassInfo) -> Result<()> {
        let vault_name = self.user_creds.vault_name.clone();

        let encrypted_shares = {
            let encryptor = MetaEncryptor {
                creds: self.user_creds.clone(),
                owner: self.vault_member.clone(),
            };
            encryptor.split_and_encrypt(pass_info.pass)?
        };

        let claim = vault_member.create_split_claim(pass_info.pass_id);

        //save meta password
        {
            let add_meta_pass = AddMetaPassEvent {
                sender: self.vault_member.member,
                meta_pass_id: claim.dist_claim_id.pass_id.clone(),
            };

            let p_device_log = PersistentDeviceLog::from(self.p_obj.clone());
            p_device_log
                .save_add_meta_pass_request(add_meta_pass)
                .await?;
        }

        {
            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
            p_ss.save_claim_in_ss_device_log(claim.clone()).await?;
        }

        for secret_share in encrypted_shares {
            let distribution_data = SecretDistributionData {
                vault_name: vault_name.clone(),
                claim_id: claim.dist_claim_id.clone(),
                secret_message: secret_share.clone(),
            };

            let dist_id = {
                let receiver = secret_share.cipher_text().channel.receiver().to_device_id();
                SsDistributionId {
                    pass_id: claim.dist_claim_id.pass_id.clone(),
                    receiver,
                }
            };

            let split_key = KvKey::from(SsWorkflowDescriptor::Distribution(dist_id));

            let ss_obj = SsWorkflowObject::Distribution(KvLogEvent {
                key: split_key.clone(),
                value: distribution_data,
            });

            self.p_obj.repo.save(ss_obj).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::common::model::vault::vault_data::VaultData;
    use crate::node::db::descriptors::vault_descriptor::DeviceLogDescriptor;
    use crate::node::db::events::vault::device_log_event::DeviceLogObject;
    use crate::node::db::events::vault::vault_log_event::{
        VaultActionEvent, VaultActionRequestEvent,
    };
    use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
    use anyhow::Result;

    #[tokio::test]
    async fn test_split_and_encrypt() -> Result<()> {
        // Setup fixture with base state
        let fixture = FixtureRegistry::base().await?;
        let creds_fixture = fixture.state.empty.user_creds;

        // Create a test password
        let password = SecretString::new("test_password".to_string().into());

        // Create member from user data
        let client_user_data = creds_fixture.client.user();
        let client_member = UserDataMember::from(client_user_data.clone());

        // Create vault data from the fixture with multiple members
        // This is crucial - MetaEncryptor needs at least 2 vault members to work properly
        let vault_data = {
            // Add additional member to ensure we have enough for shared secret distribution
            let vd_user_data = creds_fixture.vd.user();
            let vd_member = UserDataMember::from(vd_user_data);

            VaultData::from(client_member.clone())
                .update_membership(UserMembership::Member(vd_member))
        };

        // Create vault member
        let vault_member = VaultMember {
            member: client_member,
            vault: vault_data,
        };

        // Create MetaEncryptor instance
        let encryptor = MetaEncryptor {
            creds: Arc::new(creds_fixture.client),
            owner: vault_member,
        };

        // Save the number of members before calling split_and_encrypt (which moves encryptor)
        let member_count = encryptor.owner.vault.members().len();

        // Execute the split_and_encrypt function
        let encrypted_shares = encryptor.split_and_encrypt(password)?;

        // Verify the results
        assert!(
            !encrypted_shares.is_empty(),
            "Encrypted shares should not be empty"
        );
        assert_eq!(
            encrypted_shares.len(),
            member_count,
            "There should be one encrypted share per vault member"
        );

        // Verify all shares are CipherShare variants
        for share in encrypted_shares {
            assert!(
                matches!(share, EncryptedMessage::CipherShare { .. }),
                "All shares should be CipherShare variants"
            );
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_distribute_password() -> Result<()> {
        // Setup fixture with base state
        let fixture = FixtureRegistry::base().await?;
        let creds_fixture = fixture.state.empty.user_creds;

        // Create a test password and ID
        let password = SecretString::new("test_password".to_string().into());
        let password_id = MetaPasswordId::build("test_password");

        // Create member from user data
        let client_user_data = creds_fixture.client.user();
        let client_member = UserDataMember::from(client_user_data.clone());

        // Create vault data from the fixture with multiple members
        // This is crucial - MetaDistributor needs at least 2 vault members to work properly
        let vault_data = {
            // Add additional member to ensure we have enough for shared secret distribution
            let vd_user_data = creds_fixture.vd.user();
            let vd_member = UserDataMember::from(vd_user_data);

            VaultData::from(client_member.clone())
                .update_membership(UserMembership::Member(vd_member))
        };

        // Create vault member
        let vault_member = VaultMember {
            member: client_member,
            vault: vault_data,
        };

        // Create MetaDistributor with the fixture state
        let distributor = MetaDistributor {
            p_obj: fixture.state.empty.p_obj.client.clone(),
            user_creds: Arc::new(creds_fixture.client.clone()),
            vault_member: vault_member.clone(),
        };

        // Call distribute function
        let result = distributor
            .distribute(
                vault_member.clone(),
                SecurePassInfo {
                    pass_id: password_id.clone(),
                    pass: password,
                },
            )
            .await;

        // Verify distribution was successful
        assert!(result.is_ok(), "Distribution failed: {:?}", result.err());

        // Verify the claim was stored by checking the device log
        let device_log_desc = DeviceLogDescriptor::from(client_user_data.user_id());
        let device_log_events = fixture
            .state
            .empty
            .p_obj
            .client
            .get_object_events_from_beginning(device_log_desc)
            .await?;

        // Check that the meta password was added to the device log
        let found_password = device_log_events.iter().any(|event| {
            let DeviceLogObject(log_event) = event;

            let VaultActionEvent::Request(request) = &log_event.value else {
                return false;
            };

            let VaultActionRequestEvent::AddMetaPass(add_meta_pass) = request else {
                return false;
            };

            add_meta_pass.meta_pass_id.name == password_id.name
        });

        assert!(
            found_password,
            "Meta password was not added to the device log"
        );

        // Verify that a claim was generated and distributions created
        let p_ss = PersistentSharedSecret::from(fixture.state.empty.p_obj.client.clone());

        // Create a test SsClaim to check if it was stored
        let claim = vault_member.create_split_claim(password_id.clone());
        let events = p_ss.get_ss_workflow_events(claim).await?;

        // There should be at least one event for the distribution
        assert!(!events.is_empty(), "No distribution events were created");

        Ok(())
    }
}
