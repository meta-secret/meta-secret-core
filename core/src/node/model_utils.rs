use crate::crypto;
use crate::models::DeviceInfo;

impl From<String> for DeviceInfo {
    fn from(device_name: String) -> Self {
        Self {
            device_id: crypto::utils::rand_64bit_b64_url_enc().base64_text,
            device_name,
        }
    }
}
