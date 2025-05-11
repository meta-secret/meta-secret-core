use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::{UserDataMember, UserDataOutsider, UserMembership};
use crate::node::common::model::vault::vault_data::VaultAggregate;
use crate::node::db::actions::sign_up::action::SignUpAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::{VaultDescriptor, VaultStatusDescriptor};
use crate::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::Next;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, VaultActionEvent, VaultActionInitEvent, VaultActionRequestEvent, 
    VaultActionUpdateEvent
};
use crate::node::db::events::vault::vault_status::VaultStatusObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_vault::PersistentVault;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

pub struct ServerVaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> ServerVaultAction<Repo> {
    pub async fn do_processing(&self, action_event: VaultActionEvent) -> Result<()> {
        let p_vault = PersistentVault::from(self.p_obj.clone());

        //saving messages from device_log to vault_log guarantees ordering between events
        //sent from different devices simultaneously
        match &action_event {
            VaultActionEvent::Init(VaultActionInitEvent::CreateVault(create_vault_event)) => {
                let action = CreateVaultAction {
                    p_obj: self.p_obj.clone(),
                    server_device: self.server_device.clone(),
                };
                action.create(create_vault_event.owner.clone()).await?;
            }

            VaultActionEvent::Request(action_request) => {
                p_vault
                    .save_vault_log_request_event(action_request.clone())
                    .await?;

                match action_request {
                    VaultActionRequestEvent::JoinCluster(join_event) => {
                        let upd = VaultActionUpdateEvent::AddToPending {
                            candidate: join_event.candidate.clone(),
                        };
                        self.handle_update(&upd).await?;
                    }
                    VaultActionRequestEvent::AddMetaPass(add_meta_pass_event) => {
                        //server is a handler for add meta pass requests
                        let vault_name = action_request.vault_name();
                        let upd = VaultActionUpdateEvent::AddMetaPass(add_meta_pass_event.clone());

                        let vault_action_events = p_vault
                            .get_vault_log_artifact(action_event.vault_name())
                            .await?
                            .0
                            .value
                            .apply(upd.clone());

                        p_vault
                            .save_vault_log_events(vault_action_events, vault_name)
                            .await?;

                        self.handle_update(&upd).await?;
                    }
                }
            }
            VaultActionEvent::Update(action_update) => {
                self.handle_update(action_update).await?;
            }
        }

        Ok(())
    }

    async fn handle_update(&self, action_update: &VaultActionUpdateEvent) -> Result<()> {
        let p_vault = PersistentVault::from(self.p_obj.clone());
        let vault_name = action_update.vault_name();
        //check if a sender is a member of the vault and update the vault then
        let vault = p_vault.get_vault(vault_name.clone()).await?;

        let vault_action_events = p_vault
            .get_vault_log_artifact(vault_name.clone())
            .await?
            .0
            .value
            .apply(action_update.clone());

        let agg = VaultAggregate::build_from(vault_action_events, vault.clone().to_data());

        let vault_event = {
            let key = KvKey {
                obj_id: vault.obj_id().next(),
                obj_desc: VaultDescriptor::from(vault_name.clone()).to_obj_desc(),
            };
            VaultObject(KvLogEvent {
                key,
                value: agg.vault,
            })
        };

        self.p_obj.repo.save(vault_event.clone()).await?;

        p_vault
            .save_vault_log_events(agg.events, vault_name)
            .await?;

        match action_update {
            VaultActionUpdateEvent::UpdateMembership(update) => {
                self.update_vault_status(vault_event, update.update.clone()).await?;
            }
            VaultActionUpdateEvent::AddMetaPass(AddMetaPassEvent { .. }) => {
                // no extra steps required (vault  is already updated by VaultAggregate)
            }
            VaultActionUpdateEvent::AddToPending { candidate } => {
                let update = UserMembership::Outsider(UserDataOutsider::pending(candidate.clone()));
                self.update_vault_status(vault_event, update).await?;
            }
        }
        Ok(())
    }

    async fn update_vault_status(&self, vault_event: VaultObject, update: UserMembership) -> Result<()> {
        //update vault status accordingly
        let free_id = {
            let user_id = update.user_data().user_id();
            let vault_membership_desc = VaultStatusDescriptor::from(user_id);

            self.p_obj
                .find_free_id_by_obj_desc(vault_membership_desc.clone())
                .await?
        };

        let event = {
            let status = vault_event.to_data().status(update.user_data());
            let status_obj = VaultStatusObject::new(status, free_id);
            status_obj.to_generic()
        };

        self.p_obj.repo.save(event).await?;
        Ok(())
    }
}

pub struct CreateVaultAction<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
    pub server_device: DeviceData,
}

impl<Repo: KvLogEventRepo> CreateVaultAction<Repo> {
    pub async fn create(&self, owner: UserDataMember) -> Result<()> {
        // create vault if not exists
        let p_vault = PersistentVault::from(self.p_obj.clone());

        let vault_exists = p_vault.vault_exists(owner.user_data.vault_name()).await?;
        if !vault_exists {
            //create vault_log, vault and vault status
            self.create_vault(owner).await
        } else {
            // vault already exists, and the event have been saved into vault_log already,
            // no action needed
            anyhow::Ok(())
        }
    }

    async fn create_vault(&self, candidate: UserDataMember) -> Result<()> {
        //vault not found, we can create our new vault
        info!(
            "Accept SignUp request, for the vault: {:?}",
            candidate.user_data.vault_name()
        );

        let sign_up_action = SignUpAction;
        let sign_up_events = sign_up_action.accept(candidate.clone());

        for sign_up_event in sign_up_events {
            self.p_obj.repo.save(sign_up_event).await?;
        }
        anyhow::Ok(())
    }
}

/// Fixture for testing purposes
#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::states::EmptyState;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;

    pub struct ServerVaultActionFixture {
        pub server: ServerVaultAction<InMemKvLogEventRepo>,
    }

    impl ServerVaultActionFixture {
        pub fn from(state: &EmptyState) -> Self {
            Self {
                server: ServerVaultAction {
                    p_obj: state.p_obj.server.clone(),
                    server_device: state.device_creds.server.device.clone(),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::meta_tests::fixture_util::fixture::states::BaseState;
    use crate::node::common::model::meta_pass::MetaPasswordId;
    use crate::node::common::model::user::common::{UserDataMember, UserMembership};
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::events::vault::vault_log_event::{AddMetaPassEvent, UpdateMembershipEvent};
    use crate::node::db::events::vault::vault_log_event::{
        CreateVaultEvent, JoinClusterEvent, VaultActionInitEvent, VaultActionRequestEvent,
    };
    use anyhow::Result;

    #[tokio::test]
    async fn test_create_vault() -> Result<()> {
        // Setup
        let registry = FixtureRegistry::base().await?;
        create_vault(&registry).await?;

        Ok(())
    }

    async fn create_vault(registry: &FixtureRegistry<BaseState>) -> Result<()> {
        let server_vault_action = &registry.state.server_vault_action.server;
        let owner = UserDataMember::from(registry.state.empty.user_creds.client.user());

        // Create the event
        let create_vault_event =
            VaultActionInitEvent::CreateVault(CreateVaultEvent::from(owner.clone()));

        let vault_action_event = VaultActionEvent::Init(create_vault_event);

        // Act
        let result = server_vault_action.do_processing(vault_action_event).await;

        // Assert
        assert!(result.is_ok());

        // Verify vault was created
        let p_vault = PersistentVault::from(server_vault_action.p_obj.clone());
        let vault = p_vault.get_vault(owner.user_data.vault_name()).await?;

        assert!(vault.to_data().is_member(&owner.user().device.device_id));
        Ok(())
    }

    #[tokio::test]
    async fn test_add_meta_pass() -> Result<()> {
        // Setup
        let registry = FixtureRegistry::base().await?;
        let server_vault_action = &registry.state.server_vault_action.server;
        let owner = UserDataMember::from(registry.state.empty.user_creds.client.user());

        create_vault(&registry).await?;

        // Create meta pass event
        let meta_pass_event = AddMetaPassEvent {
            sender: owner.clone(),
            meta_pass_id: MetaPasswordId::build_from_str("Test Password"),
        };

        let request_event = VaultActionRequestEvent::AddMetaPass(meta_pass_event);
        let vault_action_event = VaultActionEvent::Request(request_event);

        // Act
        let result = server_vault_action.do_processing(vault_action_event).await;

        // Assert
        assert!(result.is_ok());

        // Verify meta pass was added
        let p_vault = PersistentVault::from(server_vault_action.p_obj.clone());
        let vault = p_vault.get_vault(owner.user_data.vault_name()).await?;

        assert_eq!(1, vault.to_data().secrets.len());

        Ok(())
    }

    #[tokio::test]
    async fn test_handle_vault_membership_update() -> Result<()> {
        // Setup
        let registry = FixtureRegistry::base().await?;
        let server_vault_action = &registry.state.server_vault_action.server;
        let owner = UserDataMember::from(registry.state.empty.user_creds.client.user());

        create_vault(&registry).await?;

        let new_member = UserDataMember::from(registry.state.empty.user_creds.vd.user());

        // First, we need to add a join request to the vault log
        let join_request = JoinClusterEvent::from(new_member.user_data.clone());
        let request_event = VaultActionRequestEvent::JoinCluster(join_request.clone());
        let vault_action_request = VaultActionEvent::Request(request_event);

        // Process the join request
        server_vault_action
            .do_processing(vault_action_request)
            .await?;

        // Now create the membership update event - it needs to match the request
        let update_event = VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
            sender: owner.clone(),
            update: UserMembership::Member(new_member.clone()),
            request: join_request, // Use the same join request as above
        });

        // Process the update
        let vault_action_event = VaultActionEvent::Update(update_event);
        let result = server_vault_action.do_processing(vault_action_event).await;
        assert!(result.is_ok(), "Membership update should succeed");

        // Now the new member should be properly added to the vault
        let p_vault = PersistentVault::from(server_vault_action.p_obj.clone());
        let vault = p_vault.get_vault(owner.user_data.vault_name()).await?;

        // Check if the new member was added to the vault
        let is_member = vault
            .to_data()
            .is_member(&new_member.user().device.device_id);
        assert!(
            is_member,
            "New member should be part of the vault after membership update"
        );

        // The status should now be Member
        let status = p_vault.find(new_member.user_data.clone()).await?;
        match status {
            VaultStatus::Member(_) => Ok(()),
            _ => panic!("Expected VaultStatus::Member, got {:?}", status),
        }
    }
}
