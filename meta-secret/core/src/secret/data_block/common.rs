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
        // K-of-N Secret Sharing Policy:
        // - 1 device: k=1 (trivial, no sharing)
        // - 2 devices: k=1 (full replication - each device has complete secret)
        // - 3+ devices: k=2 (Shamir Secret Sharing with threshold 2)
        match num_shares {
            0 => SharedSecretConfig {
                number_of_shares: 0,
                threshold: 0,
            },
            1 => SharedSecretConfig {
                number_of_shares: 1,
                threshold: 1,
            },
            2 => {
                // 2 devices: full replication (each has complete secret)
                SharedSecretConfig {
                    number_of_shares: 2,
                    threshold: 1,
                }
            }
            _ => {
                // 3+ devices: SSS with k=2
                SharedSecretConfig {
                    number_of_shares: num_shares,
                    threshold: 2,
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
    fn test_k_of_n_single_device() {
        // 1 device: k=1 (trivial)
        let cfg = SharedSecretConfig::calculate(1);
        assert_eq!(cfg.number_of_shares, 1);
        assert_eq!(cfg.threshold, 1);
    }

    #[test]
    fn test_k_of_n_two_devices_full_replication() {
        // 2 devices: k=1 (full replication)
        let cfg = SharedSecretConfig::calculate(2);
        assert_eq!(cfg.number_of_shares, 2);
        assert_eq!(cfg.threshold, 1);  // Each device has full secret
    }

    #[test]
    fn test_k_of_n_three_plus_devices_sss() {
        // 3+ devices: k=2 (SSS)
        for n in 3..=10 {
            let cfg = SharedSecretConfig::calculate(n);
            assert_eq!(cfg.number_of_shares, n);
            assert_eq!(cfg.threshold, 2);
        }
    }

    #[test]
    fn test_calculation_legacy() {
        // Edge case: 0 shares
        let cfg = SharedSecretConfig::calculate(0);
        assert_eq!(cfg.number_of_shares, 0);
        assert_eq!(cfg.threshold, 0);
    }
}
