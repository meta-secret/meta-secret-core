use crate::node::common::model::user::common::UserData;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::vault::vault_event::VaultObject;
use log::info;
use tracing_attributes::instrument;

pub struct SignUpAction {}

impl SignUpAction {
    #[instrument(skip(self))]
    pub fn accept(&self, candidate: UserData) -> Vec<GenericKvLogEvent> {
        info!("Create new vault");

        let mut commit_log = vec![];

        let vault_name = candidate.vault_name.clone();

        let vault_events = {
            let vault_event = VaultObject::sign_up(vault_name.clone(), candidate.clone());

            vec![vault_event.to_generic()]
        };
        commit_log.extend(vault_events);

        commit_log
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

    #[tokio::test]
    async fn test_sing_up() -> Result<()> {
        let device_creds = &DeviceCredentialsFixture::generate();
        let user_creds_fixture = UserCredentialsFixture::from(device_creds);

        let sign_up_action = SignUpAction {};
        let events = sign_up_action.accept(user_creds_fixture.client.user());

        assert_eq!(events.len(), 1);

        for event in events {
            if let GenericKvLogEvent::VaultLog(obj) = event {
                todo!("Verify events");
            }
        }

        Ok(())
    }
}
