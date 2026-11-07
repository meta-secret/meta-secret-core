//! Fixed local records. Callers hold the shared client sync owner.
use super::persistent_object::PersistentObject;
use crate::node::{
    common::model::user::common::UserId,
    db::{
        descriptors::local_guarded_preparation::{
            LocalGuardedPreparationDescriptor, LocalGuardedPreparationIndexDescriptor,
        },
        events::{
            generic_log_event::{GenericKvLogEvent, ObjIdExtractor, ToGenericEvent},
            kv_log_event::{KvKey, KvLogEvent},
            local_guarded_preparation::*,
            object_id::ArtifactId,
        },
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};
use std::sync::Arc;

#[path = "guarded_retirement_store.rs"]
mod retirement;
#[path = "guarded_rollover_store.rs"]
mod rollover;
#[path = "guarded_preparation_validation.rs"]
mod validation;
pub(crate) use retirement::CapturedRetirement;
pub(crate) use validation::local_body_commitment;
pub(crate) const UNAVAILABLE: &str = "GuardedPreparationUnavailable";

pub(crate) struct PersistentGuardedPreparation<R: KvLogEventRepo> {
    pub p_obj: Arc<PersistentObject<R>>,
}
impl<R: KvLogEventRepo> PersistentGuardedPreparation<R> {
    pub(crate) async fn load_all(
        &self,
        owner: &UserId,
    ) -> Result<Vec<LocalGuardedPreparationObject>> {
        let Some(index) = self.index(owner).await? else {
            return Ok(vec![]);
        };
        let mut records = vec![];
        for request_id in &index.0.value.request_ids {
            if let Some(record) = self.load(owner, request_id).await? {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub(crate) async fn index(
        &self,
        owner: &UserId,
    ) -> Result<Option<LocalGuardedPreparationIndexObject>> {
        let id = ArtifactId::from(LocalGuardedPreparationIndexDescriptor {
            owner: owner.clone(),
        });
        let Some(raw) = self
            .p_obj
            .repo
            .find_one(id)
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?
        else {
            return Ok(None);
        };
        let object = LocalGuardedPreparationIndexObject::try_from(raw)
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
        validation::index(&object, owner)?;
        Ok(Some(object))
    }

    pub(crate) async fn load(
        &self,
        owner: &UserId,
        request_id: &str,
    ) -> Result<Option<LocalGuardedPreparationObject>> {
        let index = self.index(owner).await?;
        let id = ArtifactId::from(LocalGuardedPreparationDescriptor {
            owner: owner.clone(),
            request_id: request_id.into(),
        });
        let Some(raw) = self
            .p_obj
            .repo
            .find_one(id)
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?
        else {
            return Ok(None);
        };
        ensure!(
            index.is_some_and(|i| i.0.value.request_ids.iter().any(|id| id == request_id)),
            UNAVAILABLE
        );
        let object = LocalGuardedPreparationObject::try_from(raw)
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
        validation::witness(&object, owner, request_id)?;
        Ok(Some(object))
    }

    async fn enroll(&self, owner: &UserId, request_id: &str) -> Result<()> {
        ensure!(
            crate::node::api::is_liveboard_request_id(request_id),
            UNAVAILABLE
        );
        let mut index = if let Some(index) = self.index(owner).await? {
            index
        } else {
            let index = LocalGuardedPreparationIndexObject(KvLogEvent {
                key: KvKey::from(LocalGuardedPreparationIndexDescriptor {
                    owner: owner.clone(),
                }),
                value: LocalGuardedPreparationIndex {
                    version: 1,
                    owner: owner.clone(),
                    request_ids: vec![],
                    revision: 0,
                },
            });
            self.write(index.clone().to_generic(), false).await?;
            index
        };
        index.0.value.revision = index
            .0
            .value
            .revision
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!(UNAVAILABLE))?;
        if !index.0.value.request_ids.iter().any(|id| id == request_id) {
            index.0.value.request_ids.push(request_id.into());
        }
        validation::index(&index, owner)?;
        // Even an already indexed UUID must prove a changed atomic replacement.
        self.write(index.to_generic(), true).await
    }

    pub(crate) async fn insert(
        &self,
        value: LocalGuardedPreparation,
    ) -> Result<LocalGuardedPreparationObject> {
        ensure!(
            self.load(&value.owner, &value.request_id).await?.is_none(),
            UNAVAILABLE
        );
        let object = LocalGuardedPreparationObject(KvLogEvent {
            key: KvKey::from(LocalGuardedPreparationDescriptor {
                owner: value.owner.clone(),
                request_id: value.request_id.clone(),
            }),
            value,
        });
        validation::witness(&object, &object.0.value.owner, &object.0.value.request_id)?;
        self.enroll(&object.0.value.owner, &object.0.value.request_id)
            .await?;
        self.write(object.clone().to_generic(), false).await?;
        Ok(object)
    }

    pub(crate) async fn replace(
        &self,
        previous: &LocalGuardedPreparationObject,
        value: LocalGuardedPreparation,
    ) -> Result<LocalGuardedPreparationObject> {
        ensure!(
            self.load(&value.owner, &value.request_id).await?.as_ref() == Some(previous),
            UNAVAILABLE
        );
        let object = LocalGuardedPreparationObject(KvLogEvent {
            key: previous.0.key.clone(),
            value,
        });
        validation::witness(&object, &object.0.value.owner, &object.0.value.request_id)?;
        self.write(object.clone().to_generic(), true).await?;
        Ok(object)
    }

    async fn write(&self, intended: GenericKvLogEvent, replace: bool) -> Result<()> {
        let id = intended.obj_id();
        let outcome = if replace {
            self.p_obj.repo.replace(intended.clone()).await
        } else {
            self.p_obj.repo.save(intended.clone()).await
        };
        let actual = self
            .p_obj
            .repo
            .find_one(id.clone())
            .await
            .map_err(|_| anyhow::anyhow!(UNAVAILABLE))?;
        ensure!(actual == Some(intended), UNAVAILABLE);
        if let Ok(saved) = outcome {
            ensure!(saved == id, UNAVAILABLE);
        }
        Ok(())
    }
}
