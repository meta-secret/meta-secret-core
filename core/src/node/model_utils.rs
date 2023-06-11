use crate::models::{DeviceInfo, MetaVault};
use crate::crypto;

impl From<String> for DeviceInfo {

    fn from(device_name: String) -> Self {
        Self {
            device_id: crypto::utils::generate_hash(),
            device_name,
        }
    }
}
