use crate::shared_secret::data_block::common;
use crate::shared_secret::data_block::common::{BlockMetaData, DataBlockParserError};

/**
 * 64 byte chunk of data that will be split by shamir secret sharing method
 */
pub const PLAIN_DATA_BLOCK_SIZE: usize = 64;

#[derive(Debug)]
pub struct PlainDataBlock {
    pub meta_data: BlockMetaData,
    pub data: [u8; PLAIN_DATA_BLOCK_SIZE],
}

impl PlainDataBlock {
    pub fn from_bytes(data: &[u8]) -> Result<Self, DataBlockParserError> {
        let meta_data = BlockMetaData { size: data.len() };

        if data == [0; PLAIN_DATA_BLOCK_SIZE] {
            return Err(DataBlockParserError::Invalid);
        }

        match meta_data.size {
            size if size == 0 || size > PLAIN_DATA_BLOCK_SIZE => {
                Err(DataBlockParserError::WrongSize)
            }

            size if size == PLAIN_DATA_BLOCK_SIZE => Ok(PlainDataBlock::new(meta_data, data)),

            size if size < PLAIN_DATA_BLOCK_SIZE => {
                let mut extended = [0; PLAIN_DATA_BLOCK_SIZE];
                extended[..data.len()].copy_from_slice(data);
                Ok(PlainDataBlock::new(meta_data, &extended))
            }

            _ => {
                panic!("Impossible")
            }
        }
    }

    pub fn new(meta_data: BlockMetaData, block_array: &[u8]) -> Self {
        Self {
            meta_data,
            data: common::parse_data::<PLAIN_DATA_BLOCK_SIZE>(block_array),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_plain_data_block_from_bytes() {
        let block = PlainDataBlock::from_bytes(&[42; 64]).unwrap();
        let expected: [u8; 64] = [42; 64];
        assert_eq!(expected, block.data);

        let block = PlainDataBlock::from_bytes(&[1; 1]).unwrap();
        let mut expected: [u8; 64] = [0; 64];
        expected[0] = 1;
        assert_eq!(1, block.meta_data.size);
        assert_eq!(expected, block.data);

        let block_err = PlainDataBlock::from_bytes(&[1; PLAIN_DATA_BLOCK_SIZE + 1]);
        assert!(block_err.is_err());
        assert_eq!(DataBlockParserError::WrongSize, block_err.unwrap_err());

        let block_err = PlainDataBlock::from_bytes(&[0; PLAIN_DATA_BLOCK_SIZE]);
        assert!(block_err.is_err());
        assert_eq!(DataBlockParserError::Invalid, block_err.unwrap_err())
    }
}
