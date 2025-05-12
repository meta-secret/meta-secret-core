use std::sync::Arc;

use crate::node::common::model::user::common::UserData;
use crate::node::common::model::vault::vault::{VaultName, VaultStatus};
use crate::node::db::descriptors::vault_descriptor::{
    VaultDescriptor, VaultLogDescriptor, VaultStatusDescriptor,
};
use crate::node::db::events::generic_log_event::KeyExtractor;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_id::{ArtifactId, Next};
use crate::node::db::events::vault::vault_event::VaultObject;
use crate::node::db::events::vault::vault_log_event::{
    VaultActionEvents, VaultActionRequestEvent, VaultLogObject,
};
use crate::node::db::events::vault::vault_status::VaultStatusObject;
use crate::node::db::objects::persistent_object::PersistentObject;
use crate::node::db::repo::generic_db::KvLogEventRepo;
use anyhow::{bail, Result};
use derive_more::From;
use tracing_attributes::instrument;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultTail {
    pub vault_log: ArtifactId,
    pub vault: ArtifactId,
    pub vault_status: ArtifactId,
}

#[derive(From)]
pub struct PersistentVault<Repo: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<Repo>>,
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_log(&self, vault_name: VaultName) -> Result<Option<VaultLogObject>> {
        //vault actions
        let vault_log_desc = VaultLogDescriptor::from(vault_name);
        let maybe_vault_log_event = self.p_obj.find_tail_event(vault_log_desc).await?;

        Ok(maybe_vault_log_event)
    }
}

impl<Repo: KvLogEventRepo> PersistentVault<Repo> {
    pub async fn vault_tail(&self, user: UserData) -> Result<VaultTail> {
        let vault_log_free_id = {
            let obj_desc = VaultLogDescriptor::from(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_free_id = {
            let obj_desc = VaultDescriptor::from(user.vault_name());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        let vault_status_free_id = {
            let obj_desc = VaultStatusDescriptor::from(user.user_id());
            self.p_obj.find_free_id_by_obj_desc(obj_desc).await?
        };

        Ok(VaultTail {
            vault_log: vault_log_free_id,
            vault: vault_free_id,
            vault_status: vault_status_free_id,
        })
    }

    pub async fn get_vault(&self, vault_name: VaultName) -> Result<VaultObject> {
        let maybe_vault_obj = self.get_vault_object(vault_name).await?;
        match maybe_vault_obj {
            None => {
                bail!("Vault not found")
            }
            Some(vault_obj) => Ok(vault_obj),
        }
    }

    /// Update membership information for a user on the server.
    #[instrument(skip_all)]
    pub async fn update_vault_membership_info_for_user(
        &self,
        user: UserData,
    ) -> Result<VaultStatus> {
        let maybe_vault_obj = self.get_vault_object(user.vault_name()).await?;
        let maybe_membership = self.get_vault_status_object(&user).await?;

        let final_status = match (maybe_vault_obj, maybe_membership) {
            // vault doesn't exist and no membership info
            (None, None) => {
                let desc = VaultStatusDescriptor::from(user.user_id());
                let curr_status = VaultStatus::NotExists(user);
                let obj = VaultStatusObject(KvLogEvent {
                    key: KvKey::from(desc),
                    value: curr_status.clone(),
                });
                self.p_obj.repo.save(obj).await?;
                curr_status
            }
            // Just in case - verify that membership is VaultNotExists
            (None, Some(status)) => {
                let not_exists = matches!(status.clone().status(), VaultStatus::NotExists(_));
                if !not_exists {
                    bail!("Invalid vault membership state. Vault already exists")
                }
                status.status()
            }
            (Some(vault_obj), None) => {
                let status = vault_obj.to_data().status(user.clone());

                let obj = VaultStatusObject(KvLogEvent {
                    key: KvKey::from(VaultStatusDescriptor::from(user.user_id())),
                    value: status.clone(),
                });
                self.p_obj.repo.save(obj).await?;
                status
            }
            // Verify that vault membership status is up to date
            (Some(vault_obj), Some(membership)) => {
                let vault_info = vault_obj.clone().to_data().status(user);
                let membership_info = membership.clone().status();

                if vault_info != membership_info {
                    let obj = VaultStatusObject(KvLogEvent {
                        key: membership.key().next(),
                        value: vault_info.clone(),
                    });
                    self.p_obj.repo.save(obj).await?;
                }

                vault_info
            }
        };

        Ok(final_status)
    }

    async fn get_vault_status_object(&self, user: &UserData) -> Result<Option<VaultStatusObject>> {
        let desc = VaultStatusDescriptor::from(user.user_id());
        self.p_obj.find_tail_event(desc.clone()).await
    }

    #[instrument(skip_all)]
    pub async fn vault_exists(&self, vault_name: VaultName) -> Result<bool> {
        let maybe_vault_obj = self.get_vault_object(vault_name).await?;
        Ok(maybe_vault_obj.is_some())
    }

    /// UserCredentials has to be created already and
    /// the sync gateway has to sync events already with the server,
    /// the server has to create a vault status table for the user
    #[instrument(skip_all)]
    pub async fn find(&self, user: UserData) -> Result<VaultStatus> {
        let maybe_vault_obj = self.get_vault_object(user.vault_name()).await?;
        let maybe_status = self.get_vault_status_object(&user).await?;

        let final_status = match (maybe_vault_obj, maybe_status) {
            (None, None) => {
                bail!(
                    "It's expected that sync with the server has happened and vault status is present"
                );
            }
            (Some(vault), None) => {
                bail!(
                    "Vault and its status have to exists together: {:?}",
                    vault.to_data().vault_name
                );
            }
            // Vault doesn't exist or the user is outsider
            (None, Some(status)) => status.status(),
            (Some(vault_obj), Some(_)) => vault_obj.to_data().status(user),
        };

        Ok(final_status)
    }

    async fn get_vault_object(&self, vault_name: VaultName) -> Result<Option<VaultObject>> {
        let desc = VaultDescriptor::from(vault_name);
        self.p_obj.find_tail_event(desc).await
    }

    pub async fn save_vault_log_events(
        &self,
        events: VaultActionEvents,
        vault_name: VaultName,
    ) -> Result<()> {
        let kv = self.get_vault_log_artifact(vault_name).await?;
        let next_key = kv.key().next();

        let vault_log_event = VaultLogObject(KvLogEvent {
            key: next_key,
            value: events,
        });

        self.p_obj.repo.save(vault_log_event).await?;

        Ok(())
    }

    pub async fn save_vault_log_request_event(
        &self,
        action_event: VaultActionRequestEvent,
    ) -> Result<()> {
        let kv = self
            .get_vault_log_artifact(action_event.vault_name())
            .await?;
        let next_key = kv.key().next();

        let vault_log_event = VaultLogObject(KvLogEvent {
            key: next_key,
            value: kv.0.value.request(action_event),
        });

        self.p_obj.repo.save(vault_log_event).await?;

        Ok(())
    }

    pub async fn get_vault_log_artifact(&self, vault_name: VaultName) -> Result<VaultLogObject> {
        let desc = VaultLogDescriptor::from(vault_name);
        let maybe_vault_log_event = self.p_obj.find_tail_event(desc.clone()).await?;

        let Some(vault_log_obj) = maybe_vault_log_event else {
            bail!("Invalid state, vault log is empty");
        };

        Ok(vault_log_obj)
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod spec {
    use crate::node::common::model::user::common::UserData;
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::objects::persistent_object::PersistentObject;
    use crate::node::db::objects::persistent_vault::PersistentVault;
    use crate::node::db::repo::generic_db::KvLogEventRepo;
    use anyhow::{bail, Result};
    use std::sync::Arc;

    pub struct VaultLogSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    pub struct VaultSpec<Repo: KvLogEventRepo> {
        pub p_obj: Arc<PersistentObject<Repo>>,
        pub user: UserData,
    }

    impl<Repo: KvLogEventRepo> VaultSpec<Repo> {
        pub async fn verify_user_is_a_member(&self) -> Result<()> {
            let p_vault = PersistentVault {
                p_obj: self.p_obj.clone(),
            };

            let vault_status = p_vault.find(self.user.clone()).await?;

            let VaultStatus::Member(_) = &vault_status else {
                bail!("Client is not a vault member: {:?}", vault_status);
            };

            Ok(())
        }
    }
}

#[cfg(any(test, feature = "test-framework"))]
pub mod fixture {
    use super::PersistentVault;
    use crate::node::db::in_mem_db::InMemKvLogEventRepo;
    use crate::node::db::objects::persistent_object::fixture::PersistentObjectFixture;
    use std::sync::Arc;

    pub struct PersistentVaultFixture {
        pub client: Arc<PersistentVault<InMemKvLogEventRepo>>,
        pub client_b: Arc<PersistentVault<InMemKvLogEventRepo>>,
        pub vd: Arc<PersistentVault<InMemKvLogEventRepo>>,
        pub server: Arc<PersistentVault<InMemKvLogEventRepo>>,
    }

    impl PersistentVaultFixture {
        pub fn generate(p_obj: &PersistentObjectFixture) -> Self {
            Self {
                client: Arc::new(PersistentVault::from(p_obj.client.clone())),
                client_b: Arc::new(PersistentVault::from(p_obj.client_b.clone())),
                vd: Arc::new(PersistentVault::from(p_obj.vd.clone())),
                server: Arc::new(PersistentVault::from(p_obj.server.clone())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::meta_tests::fixture_util::fixture::FixtureRegistry;
    use crate::node::common::model::user::common::{UserDataMember, UserDataOutsider};
    use crate::node::common::model::vault::vault::VaultStatus;
    use crate::node::db::descriptors::vault_descriptor::{
        VaultLogDescriptor, VaultStatusDescriptor,
    };
    use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
    use crate::node::db::events::object_id::ArtifactId;
    use crate::node::db::events::vault::vault_event::VaultObject;
    use crate::node::db::events::vault::vault_log_event::{
        JoinClusterEvent, VaultActionEvents, VaultActionRequestEvent, VaultLogObject,
    };
    use crate::node::db::events::vault::vault_status::VaultStatusObject;
    use crate::node::db::repo::generic_db::SaveCommand;

    #[tokio::test]
    async fn test_find_non_existent_vault_and_status() -> Result<()> {
        // Setup using FixtureRegistry
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();

        // Test that it returns the expected error when neither vault nor status exists
        let result = p_vault.find(user).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(
            "It's expected that sync with the server has happened and vault status is present"
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_find_existing_vault_no_status() -> Result<()> {
        // Setup using FixtureRegistry
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Create vault object but don't create status object
        let member = UserDataMember::from(user.clone());
        let vault_obj = VaultObject::sign_up(user.vault_name(), member);
        p_obj.repo.save(vault_obj).await?;

        // Test that it returns the expected error when vault exists but status doesn't
        let result = p_vault.find(user).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Vault and its status have to exists together")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_find_status_no_vault() -> Result<()> {
        // Setup using FixtureRegistry
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Create only status object with NotExists status
        let status = VaultStatus::NotExists(user.clone());
        let status_desc = VaultStatusDescriptor::from(user.user_id());
        let status_obj = VaultStatusObject::new(status.clone(), ArtifactId::from(status_desc));
        p_obj.repo.save(status_obj).await?;

        // Test that it returns the status when only status exists (no vault)
        let result = p_vault.find(user).await?;
        assert!(matches!(result, VaultStatus::NotExists(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_find_with_outsider_status() -> Result<()> {
        // Setup using FixtureRegistry
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Create status object with Outsider status
        let outsider = UserDataOutsider::non_member(user.clone());
        let status = VaultStatus::Outsider(outsider);
        let status_desc = VaultStatusDescriptor::from(user.user_id());
        let status_obj = VaultStatusObject::new(status, ArtifactId::from(status_desc));
        p_obj.repo.save(status_obj).await?;

        // Test that it returns the status when only status exists
        let result = p_vault.find(user).await?;
        assert!(matches!(result, VaultStatus::Outsider(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_find_with_vault_and_status() -> Result<()> {
        // Setup using FixtureRegistry
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Create a member user and vault
        let member = UserDataMember::from(user.clone());
        let vault_obj = VaultObject::sign_up(user.vault_name(), member.clone());
        p_obj.repo.save(vault_obj).await?;

        // Create status object
        let status = VaultStatus::Member(member);
        let status_desc = VaultStatusDescriptor::from(user.user_id());
        let status_obj = VaultStatusObject::new(status, ArtifactId::from(status_desc));
        p_obj.repo.save(status_obj).await?;

        // Test that it returns the member status from the vault object
        let result = p_vault.find(user).await?;
        assert!(matches!(result, VaultStatus::Member(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_vault_exists() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Test non-existent vault
        let exists = p_vault.vault_exists(user.vault_name()).await?;
        assert!(!exists);

        // Create vault and test again
        let member = UserDataMember::from(user.clone());
        let vault_obj = VaultObject::sign_up(user.vault_name(), member);
        p_obj.repo.save(vault_obj).await?;

        let exists = p_vault.vault_exists(user.vault_name()).await?;
        assert!(exists);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_vault() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Test non-existent vault
        let result = p_vault.get_vault(user.vault_name()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Vault not found"));

        // Create vault and test again
        let member = UserDataMember::from(user.clone());
        let vault_obj = VaultObject::sign_up(user.vault_name(), member);
        p_obj.repo.save(vault_obj.clone()).await?;

        let result = p_vault.get_vault(user.vault_name()).await?;
        assert_eq!(result.to_data().vault_name, user.vault_name());

        Ok(())
    }

    #[tokio::test]
    async fn test_vault_tail() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();

        let tail = p_vault.vault_tail(user).await?;

        // Verify the tail contains artifact IDs
        assert!(tail.vault_log.id.curr > 0);
        assert!(tail.vault.id.curr > 0);
        assert!(tail.vault_status.id.curr > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_vault_membership_info_for_user() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Case 1: No vault, no membership
        let status = p_vault
            .update_vault_membership_info_for_user(user.clone())
            .await?;
        assert!(matches!(status, VaultStatus::NotExists(_)));

        // Case 2: Create vault, update membership
        let member = UserDataMember::from(user.clone());
        let vault_obj = VaultObject::sign_up(user.vault_name(), member.clone());
        p_obj.repo.save(vault_obj).await?;

        let status = p_vault
            .update_vault_membership_info_for_user(user.clone())
            .await?;
        assert!(matches!(status, VaultStatus::Member(_)));

        Ok(())
    }

    #[tokio::test]
    async fn test_vault_log_operations() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Create initial vault log object with empty action events
        let desc = VaultLogDescriptor::from(user.vault_name());
        let initial_log = VaultLogObject(KvLogEvent {
            key: KvKey::from(desc.clone()),
            value: VaultActionEvents::default(),
        });
        p_obj.repo.save(initial_log).await?;

        // Test get_vault_log_artifact retrieves the log we just created
        let log_artifact = p_vault.get_vault_log_artifact(user.vault_name()).await?;
        assert_eq!(log_artifact.0.value, VaultActionEvents::default());

        // Test save_vault_log_events by creating a join request event
        let join_request = JoinClusterEvent {
            candidate: user.clone(),
        };
        let new_events = VaultActionEvents::default()
            .request(VaultActionRequestEvent::JoinCluster(join_request));
        p_vault
            .save_vault_log_events(new_events.clone(), user.vault_name())
            .await?;

        // Verify the events were saved correctly
        let updated_log = p_vault.get_vault_log_artifact(user.vault_name()).await?;
        assert_eq!(updated_log.0.value, new_events);
        assert_eq!(updated_log.0.value.requests.len(), 1);

        // Test save_vault_log_request_event by adding another join request
        // Note: This creates an identical request to the one we already added
        let join_request2 = JoinClusterEvent {
            candidate: user.clone(),
        };
        let request_event = VaultActionRequestEvent::JoinCluster(join_request2);
        p_vault.save_vault_log_request_event(request_event).await?;

        // Verify that the request count doesn't change
        // VaultActionEvents.requests is a HashSet, so identical requests are deduplicated
        let final_log = p_vault.get_vault_log_artifact(user.vault_name()).await?;
        assert_eq!(final_log.0.value.requests.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_vault_log() -> Result<()> {
        let registry = FixtureRegistry::empty();
        let user = registry.state.user_creds.client.user();
        let p_vault = registry.state.p_vault.client.clone();
        let p_obj = registry.state.p_obj.client.clone();

        // Test with non-existent vault log
        let result = p_vault.vault_log(user.vault_name()).await?;
        assert!(result.is_none());

        // Create vault log and test again
        let desc = VaultLogDescriptor::from(user.vault_name());
        let log_obj = VaultLogObject(KvLogEvent {
            key: KvKey::from(desc),
            value: VaultActionEvents::default(),
        });
        p_obj.repo.save(log_obj).await?;

        let result = p_vault.vault_log(user.vault_name()).await?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.value, VaultActionEvents::default());

        Ok(())
    }
}
