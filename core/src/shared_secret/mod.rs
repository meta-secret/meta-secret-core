use crate::{PlainText, SharedSecretConfig, SharedSecretEncryption, UserShareDto};

pub mod data_block;
pub mod shared_secret;

pub fn split(secret: String, config: SharedSecretConfig) -> Vec<UserShareDto> {
    let plain_text = PlainText::from_str(secret);
    let shared_secret = SharedSecretEncryption::new(config, &plain_text);

    let mut shares: Vec<UserShareDto> = vec![];
    for share_index in 0..config.number_of_shares {
        let share: UserShareDto = shared_secret.get_share(share_index);
        shares.push(share);
    }

    shares
}
