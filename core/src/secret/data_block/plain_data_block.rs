use crate::secret::data_block::common;
use crate::secret::data_block::common::{BlockMetaData, DataBlockParserError};

/**
 * 64 byte chunk of data that will be split by shamir secret sharing method
 */
pub const PLAIN_DATA_BLOCK_SIZE: usize = 64;

#[derive(Debug)]
pub struct PlainDataBlock {
    pub meta_data: BlockMetaData,
    pub data: [u8; PLAIN_DATA_BLOCK_SIZE],
}

impl TryFrom<&[u8]> for PlainDataBlock {
    type Error = DataBlockParserError;

    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let meta_data = BlockMetaData { size: data.len() };

        if data == [0; PLAIN_DATA_BLOCK_SIZE] {
            return Err(DataBlockParserError::Invalid);
        }

        match meta_data.size {
            size if size == PLAIN_DATA_BLOCK_SIZE => Ok(PlainDataBlock::new(meta_data, data)),

            size if size < PLAIN_DATA_BLOCK_SIZE => {
                let mut extended = [0; PLAIN_DATA_BLOCK_SIZE];
                extended[..data.len()].copy_from_slice(data);
                Ok(PlainDataBlock::new(meta_data, &extended))
            }

            wrong_size => Err(DataBlockParserError::WrongSize(wrong_size)),
        }
    }
}

impl PlainDataBlock {
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
    fn test_plain_data_block_from_bytes() -> Result<(), DataBlockParserError> {
        let data = [42; 64].as_slice();
        let block = PlainDataBlock::try_from(data)?;
        let expected: [u8; 64] = [42; 64];
        assert_eq!(expected, block.data);

        let block = PlainDataBlock::try_from([1; 1].as_slice())?;
        let mut expected: [u8; 64] = [0; 64];
        expected[0] = 1;
        assert_eq!(1, block.meta_data.size);
        assert_eq!(expected, block.data);

        let block_err = PlainDataBlock::try_from([1; PLAIN_DATA_BLOCK_SIZE + 1].as_slice());
        assert!(block_err.is_err());
        assert_eq!(
            DataBlockParserError::WrongSize(PLAIN_DATA_BLOCK_SIZE + 1),
            block_err.unwrap_err()
        );

        let block_err = PlainDataBlock::try_from([0; PLAIN_DATA_BLOCK_SIZE].as_slice());
        assert!(block_err.is_err());
        assert_eq!(DataBlockParserError::Invalid, block_err.unwrap_err());

        Ok(())
    }
}
