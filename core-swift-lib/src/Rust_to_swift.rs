use meta_secret_core::crypto::encoding::Base64EncodedText;
use meta_secret_core::crypto::key_pair::KeyPair;
use meta_secret_core::crypto::keys::{KeyManager};
use crate::Strings::RustByteSlice;

#[derive(Debug)]
pub struct KeysPair {
    transport_pub_key: Base64EncodedText,
    transport_sec_key: Base64EncodedText,
    dsa_pub_key: Base64EncodedText,
    dsa_sec_key: Base64EncodedText,
}

impl Drop for KeysPair {
    fn drop(&mut self) {
        println!("{:?} is being deallocated", self);
    }
}

#[no_mangle]
pub extern fn new_keys_pair() -> *mut KeysPair {
    let key_manager = KeyManager::generate();
    let transport_sec_key = key_manager.transport_key_pair.secret_key();
    let transport_pub_key = key_manager.transport_key_pair.public_key();
    let dsa_sec_key = key_manager.dsa.secret_key();
    let dsa_pub_key = key_manager.dsa.public_key();

    let keys_pair = KeysPair {
        transport_pub_key: transport_sec_key,
        transport_sec_key: transport_pub_key,
        dsa_pub_key,
        dsa_sec_key,
    };

    let boxed_data = Box::new(keys_pair);
    Box::into_raw(boxed_data)
}

#[no_mangle]
pub unsafe extern fn keys_pair_destroy(key_pair: *mut KeysPair) {
    let _ = Box::from_raw(key_pair);
}

#[no_mangle]
pub unsafe extern fn get_transport_pub(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport_pub_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern fn get_transport_sec(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport_sec_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern fn get_dsa_pub(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa_pub_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern fn get_dsa_sec(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa_sec_key.base64_text.as_ref())
}