//! Deliberately malformed encrypted unit records; no simulated topology lifecycle.
use super::fixture::*;
use crate::{crypto::encoding::base64::Base64Text, secret::data_block::common::SharedSecretConfig};
use anyhow::Result;
macro_rules! malformed {
    ($name:ident,$edit:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            let h = Fixture::new(2, 2).await?;
            h.edit_share($edit).await?;
            h.denied().await
        }
    };
}
malformed!(empty_share_blocks, |s| s.share_blocks.clear());
malformed!(zero_share_id, |s| s.share_id = 0);
malformed!(share_id_exceeds_stored_n, |s| s.share_id = 3);
malformed!(nonsequential_block_index, |s| s.share_blocks[1].block = 3);
malformed!(short_encoded_block, |s| s.share_blocks[0].data =
    Base64Text::from(vec![1; 112]));
malformed!(all_zero_encoded_block, |s| s.share_blocks[0].data =
    Base64Text::from(vec![0; 113]));
malformed!(zero_metadata_size, |s| s.share_blocks[0].meta_data.size = 0);
malformed!(oversize_metadata_size, |s| s.share_blocks[0]
    .meta_data
    .size = 114);
malformed!(mixed_stored_config, |s| s.share_blocks[1].config =
    SharedSecretConfig {
        number_of_shares: 1,
        threshold: 1
    });
malformed!(
    k2_cannot_supply_whole_copy,
    |s| for b in &mut s.share_blocks {
        b.config.threshold = 2;
    }
);
malformed!(
    n3_k1_cannot_supply_whole_copy,
    |s| for b in &mut s.share_blocks {
        b.config.number_of_shares = 3;
    }
);
#[tokio::test]
async fn valid_65_byte_copy_keeps_per_block_64_plus_1_metadata() -> Result<()> {
    let h = Fixture::new(2, 2).await?;
    let before = h.p.repo.get_db().await;
    let share = h
        .material()
        .local(&h.pass)
        .await?
        .expect("valid actual full copy");
    assert_eq!(
        share
            .share_blocks
            .iter()
            .map(|b| b.meta_data.size)
            .collect::<Vec<_>>(),
        vec![64, 1]
    );
    assert!(before == h.p.repo.get_db().await);
    Ok(())
}
#[tokio::test]
async fn stored_n1_is_compatible_with_current_two_members_unit_profile() -> Result<()> {
    let h = Fixture::new(1, 2).await?;
    assert!(
        h.material().local(&h.pass).await.is_ok_and(|s| s.is_some()),
        "RED: whole-copy generation N is not current N"
    );
    Ok(())
}
#[tokio::test]
async fn stored_n2_is_compatible_with_current_one_member_unit_profile() -> Result<()> {
    let h = Fixture::new(2, 1).await?;
    assert!(
        h.material().local(&h.pass).await.is_ok_and(|s| s.is_some()),
        "RED: retained full copy keeps its stored generation"
    );
    Ok(())
}
#[tokio::test]
async fn current_three_requires_strict_k2_share_and_no_local_reveal() -> Result<()> {
    let h = Fixture::new(3, 3).await?;
    assert!(h.material().local(&h.pass).await?.is_some());
    assert!(
        h.member
            .vault
            .ensure_local_secret_available(h.creds.device_id(), &h.pass)
            .is_err()
    );
    h.edit_share(|s| {
        for b in &mut s.share_blocks {
            b.config.threshold = 1;
        }
    })
    .await?;
    h.denied().await
}
