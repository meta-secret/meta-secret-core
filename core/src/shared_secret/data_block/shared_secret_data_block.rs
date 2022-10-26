use shamirsecretsharing as sss;

use crate::shared_secret::data_block::common::{BlockMetaData, SharedSecretConfig};
use crate::shared_secret::data_block::encrypted_data_block::EncryptedDataBlock;
use crate::shared_secret::data_block::plain_data_block::PlainDataBlock;

/// A PlainDataBlock (64 bytes of plain text) transformed into a shared secret
#[derive(Debug)]
pub struct SharedSecretBlock {
    pub config: SharedSecretConfig,
    pub meta_data: BlockMetaData,
    pub shares: Vec<EncryptedDataBlock>,
}

impl SharedSecretBlock {
    pub fn create(config: SharedSecretConfig, data_block: PlainDataBlock) -> SharedSecretBlock {
        let raw_shares = sss::create_shares(
            &data_block.data,
            config.number_of_shares as u8,
            config.threshold as u8,
        )
        .unwrap();

        let mut shares: Vec<EncryptedDataBlock> = vec![];
        for raw_share in raw_shares {
            let share = EncryptedDataBlock::from_bytes(&data_block.meta_data, raw_share.as_slice())
                .unwrap();

            shares.push(share);
        }

        SharedSecretBlock {
            config,
            meta_data: data_block.meta_data,
            shares,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::shared_secret::data_block::common::SharedSecretConfig;
    use crate::shared_secret::data_block::plain_data_block::PlainDataBlock;
    use crate::shared_secret::data_block::shared_secret_data_block::SharedSecretBlock;

    #[test]
    fn test_shared_secret_block() {
        let cfg = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };
        let data_block = PlainDataBlock::from_bytes(&[1, 2, 3]).unwrap();
        let shared_secret = SharedSecretBlock::create(cfg, data_block);

        println!("share1: {:?}", shared_secret.shares[0].data);
        println!("share2: {:?}", shared_secret.shares[1].data)
    }
}
