use std::borrow::Borrow;
use std::str;
use std::string::FromUtf8Error;

use crate::crypto::encoding::Base64EncodedText;
use serde::{Deserialize, Serialize};
use shamirsecretsharing::SSSError;

use crate::shared_secret::data_block::common::{BlockMetaData, SharedSecretConfig};
use crate::shared_secret::data_block::encrypted_data_block::EncryptedDataBlock;
use crate::shared_secret::data_block::plain_data_block::{PlainDataBlock, PLAIN_DATA_BLOCK_SIZE};
use crate::shared_secret::data_block::shared_secret_data_block::SharedSecretBlock;
use crate::shared_secret::shared_secret::RecoveryError::InvalidShare;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlainText {
    pub text: Vec<u8>,
}

impl PlainText {
    pub fn from_str(str: String) -> Self {
        PlainText { text: str.into_bytes() }
    }

    pub fn to_data_blocks(&self) -> Vec<PlainDataBlock> {
        self.text
            .chunks(PLAIN_DATA_BLOCK_SIZE)
            .map(|block| PlainDataBlock::from_bytes(block).unwrap())
            .collect()
    }

    pub fn as_string(&self) -> String {
        String::from_utf8(self.text.clone()).unwrap()
    }
}

//PlainText converted to shared secret
#[derive(Debug)]
pub struct SharedSecret {
    pub secret_blocks: Vec<SharedSecretBlock>,
}

// A share of the secret that user holds
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserShareDto {
    pub share_id: usize,
    pub share_blocks: Vec<SecretShareWithOrderingDto>,
}

impl UserShareDto {
    pub fn get_encrypted_data_block(&self, block_index: usize) -> EncryptedDataBlock {
        let block_dto: &SecretShareWithOrderingDto = self.share_blocks[block_index].borrow();
        block_dto.to_encrypted_data_block()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecretShareWithOrderingDto {
    pub block: usize,
    pub config: SharedSecretConfig,
    pub meta_data: BlockMetaData,
    pub data: Base64EncodedText,
}

impl SecretShareWithOrderingDto {
    pub fn to_encrypted_data_block(&self) -> EncryptedDataBlock {
        EncryptedDataBlock::from_base64(&self.meta_data, self.data.clone())
    }
}

pub struct SharedSecretEncryption;

impl SharedSecretEncryption {
    pub fn new(config: SharedSecretConfig, text: &PlainText) -> SharedSecret {
        let mut secret_blocks = vec![];
        for data_block in text.to_data_blocks() {
            let secret_block = SharedSecretBlock::create(config, data_block);
            secret_blocks.push(secret_block);
        }

        SharedSecret { secret_blocks }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("Empty input")]
    EmptyInput(String),
    #[error("Invalid share")]
    InvalidShare(String),

    #[error("Failed recover operation")]
    ShamirCombineSharesError {
        #[from]
        source: SSSError,
    },
    #[error("Non utf8 string")]
    DeserializationError {
        #[from]
        source: FromUtf8Error,
    },
}

impl SharedSecret {
    pub fn recover(self) -> Result<PlainText, RecoveryError> {
        let mut plain_text = String::new();

        let secret_blocks = self.secret_blocks;
        let size = secret_blocks.len();

        for i in 0..size {
            let secret_block: &SharedSecretBlock = secret_blocks[i].borrow();
            let shares: Vec<Vec<u8>> = secret_block.shares.iter().map(|share| share.data.to_vec()).collect();

            let maybe_restored = shamirsecretsharing::combine_shares(&shares)?;

            match maybe_restored {
                None => {
                    let err_mgs = format!("Invalid share. Secret block with index: {} has been corrupted", i);
                    return Err(InvalidShare(err_mgs));
                }
                Some(restored) => {
                    let restored: &[u8] = restored.split_at(secret_block.meta_data.size).0;

                    let restored_str = String::from_utf8(restored.to_vec())?;
                    plain_text.push_str(restored_str.as_str())
                }
            }
        }

        Ok(PlainText {
            text: plain_text.into_bytes(),
        })
    }

    pub fn get_share(&self, share_index: usize) -> UserShareDto {
        let mut share_blocks = vec![];

        for (index, curr_secret_block) in self.secret_blocks.iter().enumerate() {
            let curr_block_of_a_share = &curr_secret_block.shares[share_index];
            let share_data = SecretShareWithOrderingDto {
                block: index,
                config: curr_secret_block.config,
                meta_data: curr_secret_block.meta_data,
                data: Base64EncodedText::from(curr_block_of_a_share.data.as_slice()),
            };
            share_blocks.push(share_data);
        }

        UserShareDto {
            share_id: share_index + 1,
            share_blocks,
        }
    }
}

#[cfg(test)]
mod test {
    use std::str;

    use super::*;

    #[test]
    fn test_plain_text() {
        let text = PlainText::from_str(String::from("yay"));
        let data_blocks = text.to_data_blocks();
        assert_eq!(1, data_blocks.len());
    }

    #[test]
    fn split_and_restore_secret() {
        let mut plain_text_str = String::new();
        for i in 0..100 {
            plain_text_str.push_str(i.to_string().as_str());
            plain_text_str.push('-')
        }
        let plain_text = PlainText {
            text: plain_text_str.into_bytes(),
        };

        let secret = SharedSecretEncryption::new(
            SharedSecretConfig {
                number_of_shares: 5,
                threshold: 3,
            },
            &plain_text,
        );

        let secret_message = secret.recover().unwrap();
        assert_eq!(&plain_text.text, &secret_message.text);
        println!("message: {:?}", str::from_utf8(&secret_message.text).unwrap())
    }

    #[test]
    fn shamir_test() {
        let data: Vec<u8> = vec![
            63, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104,
            101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121,
            95, 104, 101, 121, 95, 121, 97, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121,
        ];

        let count = 5;
        let threshold = 3;
        let mut shares: Vec<Vec<u8>> = shamirsecretsharing::create_shares(&data, count, threshold).unwrap();

        for share in &shares {
            println!("share size: {:?}", share.len());
        }

        // Lose a share (for demonstration purposes)
        shares.remove(0);

        // We still have 4 shares, so we should still be able to restore the secret
        let restored = shamirsecretsharing::combine_shares(&shares).unwrap();
        assert_eq!(restored, Some(data));

        // Lose a share (for demonstration purposes)
        shares.remove(0);

        // If we lose another share the secret is lost
        shares.remove(0);
        let restored2 = shamirsecretsharing::combine_shares(&shares).unwrap();
        assert_eq!(restored2, None);
    }
}
