use crate::node::common::model::device::{DeviceCredentials, DeviceName};



pub struct ClientDeviceFixture {
    pub device: DeviceCredentials,
}

impl Default for ClientDeviceFixture {
    fn default() -> Self {
        let device = DeviceCredentials::generate(DeviceName::from("test_device"));
        Self { device }
    }
}