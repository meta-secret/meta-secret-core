use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};
use crate::CoreResult;
use crate::crypto::keys::KeyManager;
use crate::models::{AeadCipherText, EncryptedMessage, MetaPasswordDoc, MetaPasswordId, MetaPasswordRequest, SecretDistributionDocData, SecretDistributionType, UserSecurityBox, UserSignature, VaultDoc};
use crate::node::server_api;

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

            encrypted_shares.push(MetaCipherShare { receiver: receiver_sig.clone(), cipher_share: encrypted_share });
        }

        encrypted_shares
    }
}

struct MetaCipherShare {
    receiver: UserSignature,
    cipher_share: AeadCipherText,
}

pub struct MetaDistributor {
    pub security_box: UserSecurityBox,
    pub user_sig: UserSignature,
    pub vault: VaultDoc,
}

impl MetaDistributor {
    /// Encrypt and distribute password across the cluster
    pub async fn distribute(self, password_id: String, password: String) {
        let encryptor = MetaEncryptor {
            security_box: self.security_box,
            vault: self.vault.clone(),
        };

        let encrypted_shares = encryptor.encrypt(password);
        for cipher_share in encrypted_shares {
            let pass = MetaPasswordDoc {
                id: Box::new(MetaPasswordId::generate(password_id.clone())),
                vault: Box::new(self.vault.clone()),
            };

            let cipher_msg = EncryptedMessage {
                receiver: Box::from(cipher_share.receiver.clone()),
                encrypted_text: Box::new(cipher_share.cipher_share)
            };

            let distribution_share = SecretDistributionDocData {
                distribution_type: SecretDistributionType::Split,
                meta_password: Box::new(MetaPasswordRequest {
                    user_sig: Box::new(self.user_sig.clone()),
                    meta_password: Box::new(pass),
                }),
                secret_message: Box::new(cipher_msg),
            };

            server_api::distribute(&distribution_share).await.unwrap();
        }
    }
}