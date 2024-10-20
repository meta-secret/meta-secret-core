use log::info;
use tracing_attributes::instrument;
use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::UserData;
use crate::node::db::events::generic_log_event::{GenericKvLogEvent, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{UnitId, VaultGenesisEvent, VaultUnitEvent};
use crate::node::db::events::vault::vault_log_event::VaultLogObject;
use crate::node::db::events::vault_event::{VaultMembershipObject, VaultObject};

pub struct SignUpAction {}

impl SignUpAction {
    #[instrument(skip(self))]
    pub fn accept(&self, candidate: UserData, server: DeviceData) -> Vec<GenericKvLogEvent> {
        info!("Create new vault");
        
        let mut commit_log = vec![];

        let vault_name = candidate.vault_name.clone();

        let vault_log_events = {
            let unit_event = VaultLogObject::unit(vault_name.clone())
                .to_generic();
            let genesis_event = VaultLogObject::genesis(vault_name.clone(), candidate.clone())
                .to_generic();

            vec![unit_event, genesis_event]
        };
        commit_log.extend(vault_log_events);

        let vault_events = {
            let unit_event = VaultObject::unit(vault_name.clone())
                .to_generic();
            let genesis_event = VaultObject::genesis(vault_name.clone(), server)
                .to_generic();
            let vault_event = VaultObject::sign_up(vault_name.clone(), candidate.clone())
                .to_generic();

            vec![unit_event, genesis_event, vault_event]
        };
        commit_log.extend(vault_events);

        let vault_status_events = VaultMembershipObject::init(candidate);
        commit_log.extend(vault_status_events);

        commit_log
    }
}

#[cfg(test)]
mod test {
    use anyhow::Result;

    use crate::{
        node::common::model::user::user_creds::fixture::UserCredentialsFixture,
        node::db::actions::sign_up::action::SignUpAction
    };
    use crate::node::common::model::device::device_creds::fixture::DeviceCredentialsFixture;
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::vault::vault_log_event::VaultLogObject;

    #[tokio::test]
    async fn test() -> Result<()> {
        let device_creds = &DeviceCredentialsFixture::generate();
        let user_creds_fixture = UserCredentialsFixture::from(device_creds);

        let sign_up_action = SignUpAction {};
        let events = sign_up_action
            .accept(user_creds_fixture.client.user(), device_creds.server.device.clone());

        assert_eq!(events.len(), 8);

        let mut unit_event = false;
        let mut genesis_event = false;
        for event in events {
            if let GenericKvLogEvent::VaultLog(obj) = event {
                match obj {
                    VaultLogObject::Unit(_) => {
                        unit_event = true
                    }
                    VaultLogObject::Genesis(_) => {
                        genesis_event = true
                    }
                    VaultLogObject::Action(_) => {
                        //ignore
                    }
                }
            }
        }
        
        assert!(unit_event);
        assert!(genesis_event);

        Ok(())
    }
}
