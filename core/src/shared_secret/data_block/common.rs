use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq)]
pub enum DataBlockParserError {
    //Byte array has the wrong size
    WrongSize,
    //Invalid byte array
    Invalid,
}

//https://en.wikipedia.org/wiki/Shamir%27s_Secret_Sharing
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SharedSecretConfig {
    pub number_of_shares: usize,
    pub threshold: usize,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockMetaData {
    pub size: usize,
}

pub fn parse_data<const BLOCK_SIZE: usize>(block_array: &[u8]) -> [u8; BLOCK_SIZE] {
    let array_size = block_array.len();

    <[u8; BLOCK_SIZE]>::try_from(<&[u8]>::clone(&block_array))
        .expect(format!("Byte array must be the same length as a data block. Expected size: {BLOCK_SIZE}, actual {array_size}").as_str())
}
