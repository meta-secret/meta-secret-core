use crate::strings::RustByteSlice;
use meta_secret_core::crypto::encoding::serialized_key_manager::SerializedKeyManager;
use meta_secret_core::crypto::keys::KeyManager;

#[no_mangle]
pub extern "C" fn new_keys_pair() -> *mut SerializedKeyManager {
    let keys_pair = new_keys_pair_internal();

    let boxed_data = Box::new(keys_pair);
    Box::into_raw(boxed_data)
}

pub fn new_keys_pair_internal() -> SerializedKeyManager {
    let key_manager = KeyManager::generate();
    SerializedKeyManager::from(&key_manager)
}

#[no_mangle]
pub unsafe extern "C" fn keys_pair_destroy(key_pair: *mut SerializedKeyManager) {
    let _ = Box::from_raw(key_pair);
}

#[no_mangle]
pub unsafe extern "C" fn get_transport_pub(keys_pair: *const SerializedKeyManager) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport.public_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_transport_sec(keys_pair: *const SerializedKeyManager) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.transport.secret_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_dsa_pub(keys_pair: *const SerializedKeyManager) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa.public_key.base64_text.as_ref())
}

#[no_mangle]
pub unsafe extern "C" fn get_dsa_key_pair(keys_pair: *const SerializedKeyManager) -> RustByteSlice {
    let keys_pair = &*keys_pair;
    RustByteSlice::from(keys_pair.dsa.key_pair.base64_text.as_ref())
}
