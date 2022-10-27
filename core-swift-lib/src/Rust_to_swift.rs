use crate::Strings::RustByteSlice;
use meta_secret_core::crypto::encoding::Base64EncodedText;
use meta_secret_core::crypto::key_pair::KeyPair;
use meta_secret_core::crypto::keys::KeyManager;
type SizeT = usize;

#[derive(Debug)]
pub struct KeysPair {
    pub dsa_key_pair: Base64EncodedText,
    pub dsa_pub_key: Base64EncodedText,

    pub transport_pub_key: Base64EncodedText,
    pub transport_sec_key: Base64EncodedText,
}

impl Drop for KeysPair {
    fn drop(&mut self) {
        println!("{:?} is being deallocated", self);
    }
}

#[no_mangle]
pub extern "C" fn send_string_from_rust(arg: String) -> RustByteSlice {
    let s = arg;
    RustByteSlice {
        bytes: s.as_ptr(),
        len: s.len() as SizeT,
    }
}

#[no_mangle]
pub extern "C" fn new_keys_pair() -> *mut KeysPair {
    let keys_pair = new_keys_pair_internal();

    let boxed_data = Box::new(keys_pair);
    Box::into_raw(boxed_data)
}

pub fn new_keys_pair_internal() -> KeysPair {
    let key_manager = KeyManager::generate();

    let transport_sec_key = key_manager.transport_key_pair.secret_key();
    let transport_pub_key = key_manager.transport_key_pair.public_key();

    let dsa_key_pair = key_manager.dsa.encode_key_pair();
    let dsa_pub_key = key_manager.dsa.public_key();

    let keys_pair = KeysPair {
        transport_pub_key,
        transport_sec_key,
        dsa_key_pair,
        dsa_pub_key,
    };
    keys_pair
}

#[no_mangle]
pub unsafe extern "C" fn keys_pair_destroy(key_pair: *mut KeysPair) {
    let _ = Box::from_raw(key_pair);
}

#[no_mangle]
pub unsafe extern "C" fn get_transport_pub(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport_pub_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_transport_sec(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport_sec_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_dsa_pub(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa_pub_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_dsa_key_pair(keys_pair: *const KeysPair) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa_key_pair.base64_text.as_ref())
}
