use crate::strings::RustByteSlice;
use crypto_box::PublicKey as CryptoBoxPublicKey;
use crypto_box::SecretKey as CryptoBoxSecretKey;
use ed25519_dalek::Keypair as DalekKeyPair;
use meta_secret_core::crypto::encoding::Base64EncodedText;
use meta_secret_core::crypto::key_pair::{DsaKeyPair, TransportDsaKeyPair};
use meta_secret_core::crypto::keys::{AeadCipherText, KeyManager};
use meta_secret_core::shared_secret;
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret::shared_secret::UserShareDto;
use serde::{Deserialize, Serialize};
use std::slice;
use std::str;

type SizeT = usize;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMappedData {
    sender_dsa_key_pair: String,
    sender_dsa_pub_key: String,
    sender_transport_sec_key: String,
    sender_transport_pub_key: String,
    receivers_pub_keys: Vec<String>,
    secret: String,
}

#[no_mangle]
pub extern "C" fn split_and_encode(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    // Constants & Properties
    let cfg = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };

    // JSON parsing
    let json_bytes_slice = unsafe { slice::from_raw_parts(json_bytes, json_len as usize) };
    let json_string = str::from_utf8(json_bytes_slice).unwrap();
    let json_struct: JsonMappedData = serde_json::from_str(json_string).unwrap();

    let encrypted_shares_json = split_and_encode_internal(cfg, &json_struct);
    RustByteSlice {
        bytes: encrypted_shares_json.as_ptr(),
        len: encrypted_shares_json.len() as SizeT,
    }
}

fn split_and_encode_internal(cfg: SharedSecretConfig, json_struct: &JsonMappedData) -> String {
    // Split
    let top_secret_password = json_struct.secret.clone();
    let shares: Vec<UserShareDto> = shared_secret::split(top_secret_password.clone(), cfg);

    // Get DSA sender keys pair
    let sender_dsa_sec_key_decoded: Vec<u8> = base64::decode(&json_struct.sender_dsa_key_pair).unwrap();
    let dsa_keypair = DalekKeyPair::from_bytes(sender_dsa_sec_key_decoded.as_slice());

    // Generate Transport sender keys pair
    let sender_transport_sec_key_decoded: Vec<u8> = base64::decode(&json_struct.sender_transport_sec_key).unwrap();

    let sender_transport_pub_key_base64 = Base64EncodedText {
        base64_text: json_struct.sender_transport_pub_key.clone(),
    };

    // Create KeyManager
    let sender_key_manager = KeyManager {
        dsa: DsaKeyPair {
            key_pair: dsa_keypair.unwrap(),
        },
        transport_key_pair: TransportDsaKeyPair {
            secret_key: CryptoBoxSecretKey::from(
                <[u8; 32]>::try_from(sender_transport_sec_key_decoded.as_slice()).unwrap(),
            ),
            public_key: CryptoBoxPublicKey::from(&sender_transport_pub_key_base64),
        },
    };

    // Encrypt shares
    let mut encrypted_shares: Vec<AeadCipherText> = Vec::new();
    for i in 0..shares.len() {
        let password_share: &UserShareDto = &shares[i];
        let password_share: String = serde_json::to_string(&password_share).unwrap();

        let receiver_pk = Base64EncodedText {
            base64_text: json_struct.receivers_pub_keys[i].clone(),
        };

        let encrypted_share: AeadCipherText = sender_key_manager
            .transport_key_pair
            .encrypt_string(password_share, receiver_pk);

        encrypted_shares.push(encrypted_share);
    }

    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string_pretty(&encrypted_shares).unwrap();
    encrypted_shares_json
}

#[cfg(test)]
pub mod test {
    use crate::rust_to_swift::new_keys_pair_internal;
    use crate::swift_to_rust::{split_and_encode_internal, JsonMappedData};
    use meta_secret_core::crypto::key_pair::KeyPair;
    use meta_secret_core::crypto::keys::KeyManager;
    use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;

    #[test]
    fn split_and_encode() {
        let keys_pair = new_keys_pair_internal();

        // Constants & Properties
        let cfg = SharedSecretConfig {
            number_of_shares: 3,
            threshold: 2,
        };

        let km_2 = KeyManager::generate();
        let km_3 = KeyManager::generate();

        let data = JsonMappedData {
            sender_dsa_key_pair: keys_pair.dsa_key_pair.base64_text.clone(),
            sender_dsa_pub_key: keys_pair.dsa_pub_key.base64_text.clone(),
            sender_transport_sec_key: keys_pair.transport_sec_key.base64_text.clone(),
            sender_transport_pub_key: keys_pair.transport_pub_key.base64_text.clone(),
            receivers_pub_keys: vec![
                keys_pair.transport_pub_key.base64_text.clone(),
                km_2.transport_key_pair.public_key().base64_text.clone(),
                km_3.transport_key_pair.public_key().base64_text.clone(),
            ],
            secret: "top_secret".to_string(),
        };

        let json_result = split_and_encode_internal(cfg, &data);
        println!("{}", json_result);
    }
}
