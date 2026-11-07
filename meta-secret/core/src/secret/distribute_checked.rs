//! Checked receipts carry the identity selected by the actual distributor.
use super::*;
use crate::node::common::model::liveboard::{LiveboardOperationKind, LiveboardOperationReceipt};
impl<R: KvLogEventRepo> MetaDistributor<R> {
    #[instrument(skip_all)]
    pub async fn distribute_checked(
        self,
        vault_member: VaultMember,
        mut pass_info: SecurePassInfo,
    ) -> Result<crate::node::common::model::liveboard::LiveboardOperationReceipt> {
        // Recreating a deleted name starts a new identity; old offline events
        // remain bound to the tombstoned ID and cannot overwrite this Secret.
        if vault_member
            .vault
            .deleted_secrets
            .iter()
            .any(|pass| pass.id == pass_info.pass_id.id)
        {
            if vault_member
                .vault
                .secrets
                .iter()
                .any(|pass| pass.name == pass_info.pass_id.name)
            {
                anyhow::bail!("A Secret with this name already exists");
            }
            pass_info.pass_id.id = U64IdUrlEnc::from(uuid::Uuid::new_v4().to_string());
        }
        let vault_name = self.user_creds.vault_name.clone();
        let pass_id = pass_info.pass_id.clone();

        let encrypted_shares = {
            let encryptor = MetaEncryptor {
                creds: self.user_creds.clone(),
                owner: self.vault_member.clone(),
            };
            encryptor.split_and_encrypt(pass_info)?
        };

        let mut receipt = LiveboardOperationReceipt::queued(LiveboardOperationKind::Create);
        receipt.secret_id = Some(pass_id.id_str());
        let claim = vault_member.create_split_claim(pass_id);

        //save meta password
        {
            let add_meta_pass = AddMetaPassEvent {
                sender: self.vault_member.member,
                meta_pass_id: claim.dist_claim_id.pass_id.clone(),
            };

            let p_device_log = PersistentDeviceLog::from(self.p_obj.clone());
            if let Err(error) = p_device_log.save_add_meta_pass_request(add_meta_pass).await {
                if error.is::<crate::node::db::objects::persistent_object::AppendOutcomeUnknown>() {
                    return Ok(receipt.unknown("CreatePersistenceUnknown"));
                }
                anyhow::bail!("CreateNotQueued");
            }
        }

        {
            let p_ss = PersistentSharedSecret::from(self.p_obj.clone());
            if p_ss
                .save_claim_in_ss_device_log(claim.clone())
                .await
                .is_err()
            {
                return Ok(receipt.unknown("CreateIncomplete"));
            }
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

            if self.p_obj.repo.save(ss_obj).await.is_err() {
                return Ok(receipt.unknown("CreateIncomplete"));
            }
        }

        Ok(receipt)
    }
}
