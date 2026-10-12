use crate::node::common::model::device::common::DeviceData;
use crate::node::common::model::user::common::{
    UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
};
use crate::node::common::model::vault::vault_data::VaultAggregate;
use crate::node::db::actions::sign_up::action::SignUpAction;
use crate::node::db::descriptors::object_descriptor::ToObjectDescriptor;
use crate::node::db::descriptors::vault_descriptor::{VaultDescriptor, VaultStatusDescriptor};
use crate::node::db::events::generic_log_event::{ObjIdExtractor, ToGenericEvent};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::Next;
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{
    AddMetaPassEvent, DeleteSecretDecision, RemoveMemberDecision, VaultActionEvent,
    VaultActionInitEvent, VaultActionRequestEvent, VaultActionUpdateEvent,
};
use crate::node::db::events::vault::vault_status::VaultStatusObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::objects::persistent_shared_secret::PersistentSharedSecret;
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
                let vault = p_vault.get_vault(action_event.vault_name()).await?;
                match action_request {
                    VaultActionRequestEvent::JoinCluster(join_event) => {
                        let pending_request_exists = p_vault
                            .vault_log(join_event.candidate.vault_name())
                            .await?
                            .is_some_and(|log| {
                                log.0.value.requests.iter().any(|request| {
                                    matches!(request,
                                        VaultActionRequestEvent::JoinCluster(pending)
                                            if pending.candidate.device.device_id
                                                == join_event.candidate.device.device_id
                                    )
                                })
                            });
                        if pending_request_exists {
                            anyhow::bail!("A JOIN request is already pending for this device");
                        }
                        let candidate = UserDataMember {
                            user_data: join_event.candidate.clone(),
                        };
                        vault.to_data().ensure_can_add_member(&candidate)?;
                    }
                    VaultActionRequestEvent::RemoveMember(request) => {
                        vault
                            .clone()
                            .to_data()
                            .ensure_can_remove_member(request, None)?;
                        vault
                            .to_data()
                            .ensure_valid_removal_transfers(request, &request.share_transfers)?;
                    }
                    VaultActionRequestEvent::DeleteSecret(request) => {
                        let data = vault.clone().to_data();
                        data.ensure_valid_member_count()?;
                        if data.find_user(&request.sender.user_data.device.device_id)
                            != Some(&UserMembership::Member(request.sender.clone()))
                        {
                            anyhow::bail!("Deletion sender is not a current Vault member");
                        }
                        if uuid::Uuid::parse_str(&request.request_id).is_err() {
                            anyhow::bail!("Invalid deletion request ID");
                        }
                        if let Some(result) = data.deletion_results.get(&request.request_id) {
                            if result.request != *request {
                                anyhow::bail!(
                                    "Deletion request ID is already bound to another request"
                                );
                            }
                            return Ok(());
                        }
                        if data
                            .deleted_secrets
                            .iter()
                            .any(|pass| pass.id == request.pass_id.id)
                        {
                            return Ok(());
                        }
                        let mut members: Vec<_> = data
                            .members()
                            .into_iter()
                            .map(|member| member.user_data.device.device_id)
                            .collect();
                        members.sort_by_key(ToString::to_string);
                        if request.members != members {
                            // Consume an offline request from an obsolete membership
                            // without blocking the caller's canonical state refresh.
                            let mut events = p_vault
                                .get_vault_log_artifact(data.vault_name.clone())
                                .await?
                                .0
                                .value;
                            let result = DeleteSecretDecision {
                                request: request.clone(),
                                sender: request.sender.clone(),
                                accepted: false,
                            };
                            let mut canonical = vault.clone();
                            canonical.0.key.obj_id = canonical.obj_id().next();
                            canonical
                                .0
                                .value
                                .deletion_results
                                .insert(request.request_id.clone(), result.clone());
                            self.p_obj.repo.save(canonical).await?;
                            events.deletion_results.insert(result);
                            p_vault
                                .save_vault_log_events(events, data.vault_name)
                                .await?;
                            return Ok(());
                        }
                        data.ensure_can_delete_secret(request, None)?;
                        let events = p_vault
                            .get_vault_log_artifact(data.vault_name.clone())
                            .await?
                            .0
                            .value;
                        if events.deletion_results.iter().any(|result| result.request.request_id == request.request_id)
                            || events.requests.iter().any(|event| matches!(event, VaultActionRequestEvent::DeleteSecret(pending) if pending.pass_id.id == request.pass_id.id
                                && !data.deletion_results.contains_key(&pending.request_id) && data.ensure_can_delete_secret(pending, None).is_ok())) {
                            anyhow::bail!("Deletion request is already pending or terminal");
                        }
                    }
                    VaultActionRequestEvent::AddMetaPass(event) => {
                        if vault
                            .to_data()
                            .deleted_secrets
                            .iter()
                            .any(|pass| pass.id == event.meta_pass_id.id)
                        {
                            return Ok(());
                        }
                    }
                }

                p_vault
                    .save_vault_log_request_event(action_request.clone())
                    .await?;

                match action_request {
                    VaultActionRequestEvent::DeleteSecret(request) => {
                        if !request.requires_approval() {
                            self.handle_update(&VaultActionUpdateEvent::DeleteSecret(
                                DeleteSecretDecision {
                                    request: request.clone(),
                                    sender: request.sender.clone(),
                                    accepted: true,
                                },
                            ))
                            .await?;
                        }
                    }
                    VaultActionRequestEvent::JoinCluster(join_event) => {
                        let upd = VaultActionUpdateEvent::AddToPending {
                            candidate: join_event.candidate.clone(),
                        };
                        self.handle_update(&upd).await?;
                    }
                    VaultActionRequestEvent::RemoveMember(_) => {
                        // The request itself is the pending approval record.
                        // No membership mutation occurs until a signed update.
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

        if let VaultActionUpdateEvent::DeleteSecret(update) = action_update {
            if let Some(result) = vault
                .0
                .value
                .deletion_results
                .get(&update.request.request_id)
            {
                if result.request != update.request {
                    anyhow::bail!("Deletion request ID is already bound to another request");
                }
                return Ok(());
            }
            let events = p_vault
                .get_vault_log_artifact(vault_name.clone())
                .await?
                .0
                .value;
            if !events
                .requests
                .contains(&VaultActionRequestEvent::DeleteSecret(
                    update.request.clone(),
                ))
            {
                return Ok(());
            }
            vault
                .clone()
                .to_data()
                .ensure_can_delete_secret(&update.request, Some(&update.sender))?;
        }

        if let VaultActionUpdateEvent::RemoveMember(update) = action_update {
            let pending_request = VaultActionRequestEvent::RemoveMember(update.request.clone());
            let events = p_vault
                .get_vault_log_artifact(vault_name.clone())
                .await?
                .0
                .value;
            if !events.requests.contains(&pending_request) {
                info!(
                    target = %update.target.user_data.device.device_id,
                    decision = ?update.decision,
                    "Removal request is already terminal; ignoring late or duplicate response"
                );
                return Ok(());
            }
        }

        if let VaultActionUpdateEvent::UpdateMembership(update) = action_update {
            let pending_request = VaultActionRequestEvent::JoinCluster(update.request.clone());
            let events = p_vault
                .get_vault_log_artifact(vault_name.clone())
                .await?
                .0
                .value;
            if !events.requests.contains(&pending_request) {
                anyhow::bail!("JOIN request is already terminal or does not match the update");
            }
            if update.update.user_data() != update.request.candidate {
                anyhow::bail!("JOIN decision candidate does not match its request");
            }
        }

        match action_update {
            VaultActionUpdateEvent::UpdateMembership(update)
                if matches!(&update.update, UserMembership::Member(_)) =>
            {
                if let UserMembership::Member(candidate) = &update.update {
                    vault.clone().to_data().ensure_can_add_member(candidate)?;
                }
            }
            VaultActionUpdateEvent::RemoveMember(update) => {
                vault
                    .clone()
                    .to_data()
                    .ensure_can_remove_member(&update.request, Some(&update.sender))?;
                vault.clone().to_data().ensure_valid_removal_transfers(
                    &update.request,
                    &update.request.share_transfers,
                )?;
                if update.target != update.request.target {
                    anyhow::bail!("Removal response target must match its request");
                }
                match update.decision {
                    RemoveMemberDecision::Accepted => {
                        vault.clone().to_data().ensure_valid_removal_transfers(
                            &update.request,
                            &update.share_transfers,
                        )?;
                        if update.target.user_data.device.device_id != update.new_status.device_id()
                            || !matches!(
                                update.new_status,
                                UserMembership::Outsider(UserDataOutsider {
                                    status: UserDataOutsiderStatus::Declined,
                                    ..
                                })
                            )
                        {
                            anyhow::bail!("Accepted removal must remove its target");
                        }
                    }
                    RemoveMemberDecision::Declined => {
                        if !update.share_transfers.is_empty() {
                            anyhow::bail!("Declined removal must not contain Key Share transfers");
                        }
                        if update.new_status != UserMembership::Member(update.target.clone()) {
                            anyhow::bail!("Declined removal must preserve target membership");
                        }
                    }
                }
            }
            _ => {}
        }

        let vault_action_events = p_vault
            .get_vault_log_artifact(vault_name.clone())
            .await?
            .0
            .value
            .apply(action_update.clone());

        let mut agg = VaultAggregate::build_from(vault_action_events, vault.clone().to_data());
        agg.events = agg.events.retire_invalid_deletions(&agg.vault);
        for result in &agg.events.deletion_results {
            agg.vault
                .deletion_results
                .entry(result.request.request_id.clone())
                .or_insert_with(|| result.clone());
        }

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
            VaultActionUpdateEvent::DeleteSecret(_) => {
                Box::pin(
                    PersistentSharedSecret::from(self.p_obj.clone())
                        .purge_deleted_secrets(&vault_event.to_data()),
                )
                .await?;
            }
            VaultActionUpdateEvent::UpdateMembership(update) => {
                self.update_vault_status(vault_event, update.update.clone())
                    .await?;
            }
            VaultActionUpdateEvent::RemoveMember(update) => {
                if update.decision == RemoveMemberDecision::Accepted {
                    self.update_vault_status(vault_event, update.new_status.clone())
                        .await?;
                }
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

    async fn update_vault_status(
        &self,
        vault_event: VaultObject,
        update: UserMembership,
    ) -> Result<()> {
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
    use crate::node::common::model::user::common::{
        UserDataMember, UserDataOutsider, UserDataOutsiderStatus, UserMembership,
    };
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::events::vault::vault_log_event::{
        AddMetaPassEvent, RemoveMemberDecision, RemoveMemberEvent, RemoveMemberRequestEvent,
        UpdateMembershipEvent,
    };
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

    #[tokio::test]
    async fn competing_join_decision_cannot_rewrite_terminal_membership() -> Result<()> {
        let registry = FixtureRegistry::base().await?;
        let server = &registry.state.server_vault_action.server;
        let owner = UserDataMember::from(registry.state.empty.user_creds.client.user());
        let candidate = UserDataMember::from(registry.state.empty.user_creds.vd.user());
        create_vault(&registry).await?;
        let request = JoinClusterEvent::from(candidate.user_data.clone());
        server
            .do_processing(VaultActionEvent::Request(
                VaultActionRequestEvent::JoinCluster(request.clone()),
            ))
            .await?;
        server
            .do_processing(VaultActionEvent::Update(
                VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
                    request: request.clone(),
                    sender: owner,
                    update: UserMembership::Member(candidate.clone()),
                }),
            ))
            .await?;

        let stale_decline = VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(
            UpdateMembershipEvent {
                request,
                sender: UserDataMember::from(registry.state.empty.user_creds.client.user()),
                update: UserMembership::Outsider(UserDataOutsider {
                    user_data: candidate.user_data.clone(),
                    status: UserDataOutsiderStatus::Declined,
                }),
            },
        ));
        let result = server.do_processing(stale_decline).await;
        let status = PersistentVault::from(server.p_obj.clone())
            .find(candidate.user_data.clone())
            .await?;
        assert!(
            result.is_err(),
            "a competing decision must be rejected after the JOIN is terminal"
        );
        assert!(
            matches!(status, VaultStatus::Member(_)),
            "stale decline must not revoke accepted membership"
        );
        Ok(())
    }

    #[tokio::test]
    async fn declined_removal_blocks_late_accept() -> Result<()> {
        let registry = FixtureRegistry::base().await?;
        let server = &registry.state.server_vault_action.server;
        let owner = UserDataMember::from(registry.state.empty.user_creds.client.user());
        let approver = UserDataMember::from(registry.state.empty.user_creds.client_b.user());
        let target = UserDataMember::from(registry.state.empty.user_creds.vd.user());
        create_vault(&registry).await?;

        for candidate in [&approver, &target] {
            let request = JoinClusterEvent::from(candidate.user_data.clone());
            server
                .do_processing(VaultActionEvent::Request(
                    VaultActionRequestEvent::JoinCluster(request.clone()),
                ))
                .await?;
            server
                .do_processing(VaultActionEvent::Update(
                    VaultActionUpdateEvent::UpdateMembership(UpdateMembershipEvent {
                        request,
                        sender: owner.clone(),
                        update: UserMembership::Member(candidate.clone()),
                    }),
                ))
                .await?;
        }

        let request = RemoveMemberRequestEvent {
            sender: owner.clone(),
            target: target.clone(),
            share_transfers: vec![],
        };
        server
            .do_processing(VaultActionEvent::Request(
                VaultActionRequestEvent::RemoveMember(request.clone()),
            ))
            .await?;

        server
            .do_processing(VaultActionEvent::Update(
                VaultActionUpdateEvent::RemoveMember(RemoveMemberEvent {
                    request: request.clone(),
                    sender: approver.clone(),
                    target: target.clone(),
                    new_status: UserMembership::Member(target.clone()),
                    decision: RemoveMemberDecision::Declined,
                    share_transfers: vec![],
                }),
            ))
            .await?;

        let p_vault = PersistentVault::from(server.p_obj.clone());
        let after_decline = p_vault
            .get_vault(owner.user_data.vault_name())
            .await?
            .to_data();
        assert_eq!(after_decline.members().len(), 3);
        assert!(after_decline.is_member(&target.user().device.device_id));

        server
            .do_processing(VaultActionEvent::Update(
                VaultActionUpdateEvent::RemoveMember(RemoveMemberEvent {
                    request,
                    sender: approver,
                    target: target.clone(),
                    new_status: UserMembership::Outsider(UserDataOutsider {
                        user_data: target.user_data.clone(),
                        status: UserDataOutsiderStatus::Declined,
                    }),
                    decision: RemoveMemberDecision::Accepted,
                    share_transfers: vec![],
                }),
            ))
            .await?;

        let after_late_accept = p_vault
            .get_vault(owner.user_data.vault_name())
            .await?
            .to_data();
        assert_eq!(after_late_accept.members().len(), 3);
        assert!(after_late_accept.is_member(&target.user().device.device_id));
        assert!(matches!(
            p_vault.find(target.user_data).await?,
            VaultStatus::Member(_)
        ));
        Ok(())
    }
}
