use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum DataBlockParserError {
    #[error("Byte array has wrong size: {0}")]
    WrongSize(usize),
    #[error("Invalid (empty) byte array")]
    Invalid,
}

//https://en.wikipedia.org/wiki/Shamir%27s_Secret_Sharing
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SharedSecretConfig {
    pub number_of_shares: usize,
    pub threshold: usize,
}

impl SharedSecretConfig {
    pub fn calculate(num_shares: usize) -> Self {
        match num_shares {
            1 | 2 => {
                SharedSecretConfig {
                    number_of_shares: num_shares,
                    threshold: 1,
                }
            }
            n if n % 2 == 0 => {
                let half = num_shares / 2;
                SharedSecretConfig {
                    number_of_shares: num_shares,
                    threshold: half,
                }
            }
            _ => {
                let quorum = (num_shares / 2) + 1;
                SharedSecretConfig {
                    number_of_shares: num_shares,
                    threshold: quorum,
                }
            }
        }
    }
}

impl Default for SharedSecretConfig {
    fn default() -> Self {
        Self {
            number_of_shares: 3,
            threshold: 2,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockMetaData {
    pub size: usize,
}

pub fn parse_data<const BLOCK_SIZE: usize>(block_array: &[u8]) -> [u8; BLOCK_SIZE] {
    let array_size = block_array.len();

    <[u8; BLOCK_SIZE]>::try_from(<&[u8]>::clone(&block_array)).expect(
        format!("Byte array must be the same length as a data block. Expected size: {BLOCK_SIZE}, actual {array_size}")
            .as_str(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_calculation() {
        let cfg = SharedSecretConfig::calculate(1);
        assert_eq!(cfg.number_of_shares, 1);
        assert_eq!(cfg.threshold, 1);

        let cfg = SharedSecretConfig::calculate(2);
        assert_eq!(cfg.number_of_shares, 2);
        assert_eq!(cfg.threshold, 1);

        let cfg = SharedSecretConfig::calculate(3);
        assert_eq!(cfg.number_of_shares, 3);
        assert_eq!(cfg.threshold, 2);

        let cfg = SharedSecretConfig::calculate(4);
        assert_eq!(cfg.number_of_shares, 4);
        assert_eq!(cfg.threshold, 2);

        let cfg = SharedSecretConfig::calculate(5);
        assert_eq!(cfg.number_of_shares, 5);
        assert_eq!(cfg.threshold, 3);
    }
}