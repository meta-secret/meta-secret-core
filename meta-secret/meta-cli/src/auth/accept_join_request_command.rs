use crate::base_command::BaseCommand;
use anyhow::Result;

pub struct AcceptJoinRequestCommand {
    pub base: BaseCommand,
    pub claim_id: String,
}

impl AcceptJoinRequestCommand {
    pub fn new(db_name: String, claim_id: String) -> Self {
        Self {
            base: BaseCommand::new(db_name),
            claim_id,
        }
    }

    pub async fn execute(&self) -> Result<()> {
        // TODO: Implement accept join request functionality
        todo!("Implement accept join request functionality")
    }
} 