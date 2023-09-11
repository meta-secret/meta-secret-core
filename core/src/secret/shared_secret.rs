use std::borrow::Borrow;
use std::str;

use serde::{Deserialize, Serialize};

use crate::CoreResult;
use crate::errors::{CoreError, RecoveryError};
use crate::errors::RecoveryError::InvalidShare;
use crate::models::Base64EncodedText;
use crate::secret::data_block::common::{BlockMetaData, SharedSecretConfig};
use crate::secret::data_block::encrypted_data_block::EncryptedDataBlock;
use crate::secret::data_block::plain_data_block::{PLAIN_DATA_BLOCK_SIZE, PlainDataBlock};
use crate::secret::data_block::shared_secret_data_block::SharedSecretBlock;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlainText {
    pub text: String,
}

impl PlainText {
    pub fn to_data_blocks(&self) -> Vec<PlainDataBlock> {
        self.text
            .clone()
            .into_bytes()
            .chunks(PLAIN_DATA_BLOCK_SIZE)
            .map(|block| PlainDataBlock::try_from(block).unwrap())
            .collect()
    }
}

impl From<String> for PlainText {
    fn from(data: String) -> Self {
        Self { text: data }
    }
}

impl From<&str> for PlainText {
    fn from(str: &str) -> Self {
        PlainText::from(str.to_string())
    }
}

impl ToString for PlainText {
    fn to_string(&self) -> String {
        self.text.clone()
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
    pub fn get_encrypted_data_block(&self, block_index: usize) -> CoreResult<EncryptedDataBlock> {
        let block_dto = &self.share_blocks[block_index];
        let data_block = EncryptedDataBlock::try_from(block_dto)?;
        Ok(data_block)
    }
}

impl TryFrom<&Base64EncodedText> for UserShareDto {
    type Error = CoreError;

    fn try_from(base64_content: &Base64EncodedText) -> Result<Self, Self::Error> {
        let data = Vec::try_from(base64_content)?;
        let json = serde_json::from_slice(data.as_slice())?;
        Ok(json)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SecretShareWithOrderingDto {
    pub block: usize,
    pub config: SharedSecretConfig,
    pub meta_data: BlockMetaData,
    pub data: Base64EncodedText,
}

impl TryFrom<&SecretShareWithOrderingDto> for EncryptedDataBlock {
    type Error = CoreError;

    fn try_from(share: &SecretShareWithOrderingDto) -> Result<Self, Self::Error> {
        let data_vec = Vec::try_from(&share.data)?;
        let data_bytes = data_vec.as_slice();
        let block = EncryptedDataBlock::from_bytes(&share.meta_data, data_bytes)?;
        Ok(block)
    }
}

pub struct SharedSecretEncryption;

impl SharedSecretEncryption {
    pub fn new(config: SharedSecretConfig, text: &PlainText) -> CoreResult<SharedSecret> {
        let mut secret_blocks = vec![];
        for data_block in text.to_data_blocks() {
            let secret_block = SharedSecretBlock::create(config, data_block)?;
            secret_blocks.push(secret_block);
        }

        Ok(SharedSecret { secret_blocks })
    }
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

        Ok(PlainText::from(plain_text))
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
    use shamirsecretsharing::SSSError;

    use super::*;

    #[test]
    fn test_plain_text() {
        let text = PlainText::from("yay");
        let data_blocks = text.to_data_blocks();
        assert_eq!(1, data_blocks.len());
    }

    #[test]
    fn split_and_restore_secret() -> CoreResult<()> {
        let mut plain_text_str = String::new();
        for i in 0..100 {
            plain_text_str.push_str(i.to_string().as_str());
            plain_text_str.push('-')
        }
        let plain_text = PlainText::from(plain_text_str);

        let secret = SharedSecretEncryption::new(
            SharedSecretConfig {
                number_of_shares: 5,
                threshold: 3,
            },
            &plain_text,
        )?;

        let secret_message = secret.recover()?;
        assert_eq!(&plain_text.text, &secret_message.text);
        println!("message: {:?}", &secret_message.text);

        Ok(())
    }

    #[test]
    fn shamir_test() -> Result<(), SSSError> {
        let data: Vec<u8> = vec![
            63, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104,
            101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121,
            95, 104, 101, 121, 95, 121, 97, 121, 95, 104, 101, 121, 95, 104, 101, 121, 95, 104, 101, 121,
        ];

        let count = 5;
        let threshold = 3;
        let mut shares: Vec<Vec<u8>> = shamirsecretsharing::create_shares(&data, count, threshold)?;

        for share in &shares {
            println!("share size: {:?}", share.len());
        }

        // Lose a share (for demonstration purposes)
        shares.remove(0);

        // We still have 4 shares, so we should still be able to restore the secret
        let restored = shamirsecretsharing::combine_shares(&shares)?;
        assert_eq!(restored, Some(data));

        // Lose a share (for demonstration purposes)
        shares.remove(0);

        // If we lose another share the secret is lost
        shares.remove(0);
        let restored2 = shamirsecretsharing::combine_shares(&shares)?;
        assert_eq!(restored2, None);

        Ok(())
    }
}
