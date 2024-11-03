use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::secret::data_block::common;
use crate::secret::data_block::common::{BlockMetaData, DataBlockParserError};

pub const SECRET_DATA_BLOCK_SIZE: usize = 113;

//block of data after converting PlainDataBlock to a shared secret
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncryptedDataBlock {
    #[serde(with = "BigArray")]
    pub data: [u8; SECRET_DATA_BLOCK_SIZE],
}

impl EncryptedDataBlock {
    pub fn from_bytes(
        meta_data: &BlockMetaData,
        data: &[u8],
    ) -> Result<Self, DataBlockParserError> {
        // An array can't be empty
        if data == [0; SECRET_DATA_BLOCK_SIZE] {
            return Err(DataBlockParserError::Invalid);
        }

        if meta_data.size == 0 || meta_data.size > SECRET_DATA_BLOCK_SIZE {
            return Err(DataBlockParserError::WrongSize(meta_data.size));
        }

        let share = Self {
            data: common::parse_data::<SECRET_DATA_BLOCK_SIZE>(data),
        };

        Ok(share)
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::encoding::base64::Base64Text;
    use crate::errors::CoreError;
    use crate::secret::data_block::common::BlockMetaData;
    use crate::secret::data_block::common::SharedSecretConfig;
    use crate::secret::data_block::encrypted_data_block::{
        EncryptedDataBlock, SECRET_DATA_BLOCK_SIZE,
    };
    use crate::secret::shared_secret::SecretShareWithOrderingDto;

    #[test]
    fn test_encrypted_data_block() -> Result<(), CoreError> {
        let meta_data = BlockMetaData { size: 11 };
        let raw_data = vec![1; SECRET_DATA_BLOCK_SIZE];
        let text = Base64Text::from(raw_data.clone());

        let secret_share_dto = SecretShareWithOrderingDto {
            block: 0,
            config: SharedSecretConfig {
                number_of_shares: 3,
                threshold: 2,
            },
            meta_data,
            data: text,
        };

        let secret = EncryptedDataBlock::try_from(&secret_share_dto)?;
        assert_eq!(raw_data.as_slice(), secret.data.as_slice());

        Ok(())
    }
}
