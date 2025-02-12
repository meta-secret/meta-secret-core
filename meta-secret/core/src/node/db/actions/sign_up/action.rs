use crate::node::common::model::user::common::{UserData, UserDataMember};
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::vault::vault_event::VaultObject;
use log::info;
use tracing_attributes::instrument;
use crate::node::db::events::vault::vault_log_event::VaultLogObject;

pub struct SignUpAction;

impl SignUpAction {
    #[instrument(skip(self))]
    pub fn accept(&self, candidate: UserDataMember) -> Vec<GenericKvLogEvent> {
        info!("Create new vault");
        
        let vault_name = candidate.user_data.vault_name();
        
        let vault_log_event = VaultLogObject::create(candidate.clone()).to_generic();

        let vault_event = {
            let vault_event = VaultObject::sign_up(vault_name.clone(), candidate);
            vault_event.to_generic()
        };
        
        vec![vault_log_event, vault_event]
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::{
        node::common::model::user::user_creds::fixture::UserCredentialsFixture,
        node::db::actions::sign_up::action::SignUpAction,
    };
    use crate::node::common::model::user::common::UserDataMember;

    #[tokio::test]
    async fn test_sing_up() -> Result<()> {
        let device_creds = &DeviceCredentialsFixture::generate();
        let user_creds_fixture = UserCredentialsFixture::from(device_creds);

        let sign_up_action = SignUpAction;
        let events = sign_up_action.accept(UserDataMember::from(user_creds_fixture.client.user()));

        assert_eq!(events.len(), 2);

        for event in events {
            if let GenericKvLogEvent::VaultLog(_obj) = event {
                //todo!("Verify events");
            }
        }

        Ok(())
    }
}
