use crate::base_command::BaseCommand;
use anyhow::Result;
use meta_secret_core::node::app::meta_app::messaging::GenericAppStateRequest;

pub struct RemoveDeviceCommand {
    pub base: BaseCommand,
    pub device_id: String,
}

impl RemoveDeviceCommand {
    pub fn new(db_name: String, device_id: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            device_id,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        let db_context = self.base.open_existing_db().await?;
        self.base
            .handle_client_request(
                &db_context,
                GenericAppStateRequest::RemoveMember(self.device_id.clone()),
            )
            .await?;
        println!("Removal request submitted for device {}", self.device_id);
        Ok(())
    }
}
