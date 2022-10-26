use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::crypto::encoding::Base64EncodedText;
use crate::shared_secret::data_block::common;
use crate::shared_secret::data_block::common::{BlockMetaData, DataBlockParserError};

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
            return Err(DataBlockParserError::WrongSize);
        }

        let share = Self {
            data: common::parse_data::<SECRET_DATA_BLOCK_SIZE>(data),
        };

        Ok(share)
    }

    pub fn from_base64(meta_data: &BlockMetaData, data: Base64EncodedText) -> EncryptedDataBlock {
        let data_vec: Vec<u8> = data.into();
        let data: &[u8] = data_vec.as_slice();

        EncryptedDataBlock::from_bytes(meta_data, data).unwrap()
    }
}

#[cfg(test)]
mod test {
    use crate::crypto::encoding::Base64EncodedText;
    use crate::shared_secret::data_block::common::BlockMetaData;
    use crate::shared_secret::data_block::encrypted_data_block::{
        EncryptedDataBlock, SECRET_DATA_BLOCK_SIZE,
    };

    #[test]
    fn test_encrypted_data_block() {
        let meta_data = BlockMetaData { size: 11 };
        let raw_data = vec![1; SECRET_DATA_BLOCK_SIZE];
        let text = Base64EncodedText::from(raw_data.clone());

        let secret = EncryptedDataBlock::from_base64(&meta_data, text);
        assert_eq!(raw_data.as_slice(), secret.data.as_slice());
    }
}
