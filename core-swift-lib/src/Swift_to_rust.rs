use std::str;
use std::slice;
use meta_secret_core::crypto::encoding::Base64EncodedText;
use meta_secret_core::crypto::key_pair::{DsaKeyPair, TransportDsaKeyPair};
use meta_secret_core::crypto::keys::{AeadCipherText, KeyManager};
use meta_secret_core::shared_secret::data_block::common::SharedSecretConfig;
use meta_secret_core::shared_secret;
use meta_secret_core::shared_secret::shared_secret::UserShareDto;
use serde::{Deserialize, Serialize};
use ed25519_dalek::{Keypair as DalekKeyPair};
use crypto_box::{KEY_SIZE, PublicKey as CryptoBoxPublicKey};
use crypto_box::SecretKey as CryptoBoxSecretKey;
use crate::Strings::RustByteSlice;

type SizeT = usize;

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JsonMappedData {
    sender_dsa_sec_key: String,
    sender_dsa_pub_key: String,
    sender_transport_sec_key: String,
    sender_transport_pub_key: String,
    receivers_pub_keys: Vec<String>,
    secret: String
}

#[no_mangle]
pub extern fn split_and_encode(json_bytes: *const u8, json_len: SizeT) -> RustByteSlice {
    // Constants & Properties
    let cfg = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };

    // JSON parsing
    let json_bytes_slice = unsafe { slice::from_raw_parts(json_bytes, json_len as usize) };
    let json_string = str::from_utf8(json_bytes_slice).unwrap();
    let json_struct: JsonMappedData = serde_json::from_str(json_string).unwrap();

    // Split
    let top_secret_password = json_struct.secret;
    let shares: Vec<UserShareDto> = shared_secret::split(top_secret_password.clone(), cfg);

    // Generate DSA sender keys pair
    let sender_dsa_sec_key_decoded = base64::decode(&json_struct.sender_dsa_sec_key).unwrap();
    let dsa_keypair = DalekKeyPair::from_bytes(sender_dsa_sec_key_decoded.as_slice());

    // Generate Transport sender keys pair
    let sender_transport_sec_key_decoded: Vec<u8> = base64::decode(&json_struct.sender_transport_sec_key).unwrap();
    let sender_transport_pub_key_base64 = Base64EncodedText::from(json_struct.sender_transport_pub_key.as_bytes());

    // Create KeyManager
    let sender_key_manager = KeyManager {
        dsa: DsaKeyPair { key_pair: dsa_keypair.unwrap() },
        transport_key_pair: TransportDsaKeyPair {
            secret_key: CryptoBoxSecretKey::from(<[u8; 32]>::try_from(sender_transport_sec_key_decoded.as_slice()).unwrap()),
            public_key: CryptoBoxPublicKey::from(&sender_transport_pub_key_base64) }
    };

    // Encrypt shares
    let mut encrypted_shares: Vec<AeadCipherText> = Vec::new();
    for i in 1..shares.len() {
        let password_share: &UserShareDto = &shares[i];

        let encrypted_share: AeadCipherText = sender_key_manager.transport_key_pair.encrypt_string(
            serde_json::to_string(&password_share).unwrap(),
            Base64EncodedText::from(json_struct.receivers_pub_keys[i].as_bytes()) ,
        );

        encrypted_shares.push(encrypted_share);
    }

    // Shares to JSon
    let encrypted_shares_json = serde_json::to_string(&encrypted_shares).unwrap();
    RustByteSlice{
        bytes: encrypted_shares_json.as_ptr(),
        len: encrypted_shares_json.len() as SizeT,
    }
}