//! Return-only proof faults; the physical store and every write stay unchanged.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    MissingPrepared,
    WrongBase,
    SignedFull,
}

#[derive(Clone)]
pub(super) struct Hit {
    pub order: usize,
    pub id: ArtifactId,
    pub original: GenericKvLogEvent,
    pub returned: Option<GenericKvLogEvent>,
}

pub(super) struct Repo {
    pub inner: Arc<repo::Repo>,
    pub mode: Mode,
    pub old: LocalGuardedPreparationObject,
    pub before: VaultObject,
    pub full: LocalGuardedPreparationObject,
    pub request_id: ArtifactId,
    pub hits: Mutex<Vec<Hit>>,
    pub enabled: std::sync::atomic::AtomicBool,
}

impl Repo {
    async fn imported_singleton(&self) -> Result<bool> {
        let Some(event) = self
            .inner
            .physical
            .find_one(self.before.obj_id().next())
            .await?
        else {
            return Ok(false);
        };
        let vault = event.vault()?;
        let owner = &self.old.0.value.owner;
        let survivors = self.before.0.value.members();
        let a = survivors
            .iter()
            .find(|m| m.user_data.user_id() == *owner)
            .context("SETUP exact original owner")?;
        let b = survivors
            .iter()
            .find(|m| m.user_data.user_id() != *owner)
            .context("SETUP actual other survivor")?;
        Ok(vault.obj_id() == self.before.obj_id().next()
            && vault.0.value.members() == vec![(*a).clone()]
            && matches!(vault.0.value.find_user(&b.user_data.device.device_id),
                Some(UserMembership::Outsider(o)) if o.user_data == b.user_data
                    && o.status == UserDataOutsiderStatus::Declined))
    }

    async fn returned_fault(
        &self,
        event: &GenericKvLogEvent,
    ) -> Result<Option<Option<GenericKvLogEvent>>> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(None);
        }
        if !self.imported_singleton().await? {
            return Ok(None);
        }
        if self.mode == Mode::SignedFull {
            return Ok((event == &self.old.clone().to_generic())
                .then(|| Some(self.full.clone().to_generic())));
        }
        let GenericKvLogEvent::VaultLog(log) = event else {
            return Ok(None);
        };
        let stream = ArtifactId::from(
            meta_secret_core::node::db::descriptors::vault_descriptor::VaultLogDescriptor::from(
                self.old.0.value.owner.vault_name.clone(),
            ),
        )
        .fqdn;
        if log.obj_id().fqdn != stream {
            return Ok(None);
        }
        let r = self
            .inner
            .physical
            .find_one(self.request_id.clone())
            .await?
            .context("actual R before proof fault")?
            .device_log()?;
        let u = self
            .inner
            .physical
            .find_one(self.request_id.clone().next())
            .await?
            .context("actual U before proof fault")?
            .device_log()?;
        let (
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)),
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
        ) = (&r.0.value, &u.0.value)
        else {
            anyhow::bail!("actual R/U types before proof fault")
        };
        if !log
            .0
            .value
            .liveboard_removal_applications
            .iter()
            .any(|p| p.update == *update && p.update.request == *request)
        {
            return Ok(None);
        }
        if self.mode == Mode::MissingPrepared {
            return Ok(Some(None));
        }
        let mut changed = log.clone();
        for receipt in &mut changed.0.value.liveboard_removal_applications {
            if receipt.update == *update && receipt.update.request == *request {
                ensure!(
                    receipt.base_vault_artifact_id == self.before.obj_id(),
                    "actual Prepared base before fault differs"
                );
                receipt.base_vault_artifact_id = self.old.0.value.base_vault.clone();
            }
        }
        Ok(Some(Some(changed.to_generic())))
    }

    pub fn evidence(&self) -> Result<Hit> {
        let hits = self.hits.lock().unwrap();
        let first = hits
            .first()
            .context("LATE selected proof read UNREACHED; not a passing negative")?;
        for hit in hits.iter() {
            ensure!(
                hit.id == hit.original.obj_id(),
                "outer selected fullID differs"
            );
            match self.mode {
                Mode::MissingPrepared => ensure!(
                    matches!(&hit.original, GenericKvLogEvent::VaultLog(_))
                        && hit.returned.is_none(),
                    "outer missing-Prepared return not proved"
                ),
                Mode::WrongBase => {
                    let mut intended = hit.original.clone().vault_log()?;
                    let actual = hit
                        .returned
                        .clone()
                        .context("outer wrong-base return absent")?
                        .vault_log()?;
                    for receipt in &mut intended.0.value.liveboard_removal_applications {
                        if receipt.base_vault_artifact_id == self.before.obj_id() {
                            receipt.base_vault_artifact_id = self.old.0.value.base_vault.clone();
                        }
                    }
                    ensure!(
                        actual == intended && actual.to_generic() != hit.original,
                        "outer wrong-base exact returned clone not proved"
                    );
                }
                Mode::SignedFull => ensure!(
                    hit.original == self.old.clone().to_generic()
                        && hit.returned.as_ref() == Some(&self.full.clone().to_generic()),
                    "outer actual Signed/FULL return not proved"
                ),
            }
        }
        Ok(first.clone())
    }
}

#[async_trait(?Send)]
impl FindOneQuery for Repo {
    async fn find_one(&self, id: ArtifactId) -> Result<Option<GenericKvLogEvent>> {
        let original = self.inner.find_one(id.clone()).await?;
        let Some(event) = &original else {
            return Ok(original);
        };
        let Some(returned) = self.returned_fault(event).await? else {
            return Ok(original);
        };
        // The inner observer records the physical original. This outer ledger records
        // the exact value actually returned to the production caller instead.
        self.hits.lock().unwrap().push(Hit {
            order: self.inner.tick(),
            id,
            original: event.clone(),
            returned: returned.clone(),
        });
        Ok(returned)
    }
    async fn get_key(&self, id: ArtifactId) -> Result<Option<ArtifactId>> {
        self.inner.get_key(id).await
    }
}
#[async_trait(?Send)]
impl SaveCommand for Repo {
    async fn save<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.inner.save(event).await
    }
    async fn replace<T: ToGenericEvent>(&self, event: T) -> Result<ArtifactId> {
        self.inner.replace(event).await
    }
}
#[async_trait(?Send)]
impl DeleteCommand for Repo {
    async fn delete(&self, id: ArtifactId) {
        self.inner.delete(id).await
    }
    async fn try_delete(&self, id: ArtifactId) -> Result<()> {
        self.inner.try_delete(id).await
    }
}
#[async_trait(?Send)]
impl DbCleanUpCommand for Repo {
    async fn db_clean_up(&self) {
        self.inner.db_clean_up().await
    }
}
impl KvLogEventRepo for Repo {
    fn server_recovery_response_store(
        &self,
    ) -> Result<
        &dyn meta_secret_core::node::db::repo::recovery_response_store::ServerRecoveryResponseStore,
    > {
        self.inner.server_recovery_response_store()
    }
}

pub(super) fn no_effects(
    h: &harness::Harness,
    old: &LocalGuardedPreparationObject,
    r: &DeviceLogObject,
    u: &DeviceLogObject,
    start: usize,
    hit: usize,
    returned: usize,
) -> Result<()> {
    {
        let calls = h.repo.calls.lock().unwrap();
        for call in calls
            .iter()
            .filter(|c| c.order > start && c.order < returned)
        {
            if let repo::Op::Write(_, event) = &call.op {
                ensure!(
                    !matches!(&**event, GenericKvLogEvent::DeviceTransportNoop(_)),
                    "invalid proof attempted local Noop write"
                );
                if matches!(&**event, GenericKvLogEvent::DeviceLog(_)) {
                    ensure!(
                        call.order < hit
                            && (**event == r.clone().to_generic()
                                || **event == u.clone().to_generic()),
                        "invalid proof attempted new or resubmitted local command"
                    );
                }
                ensure!(
                    !matches!(&**event, GenericKvLogEvent::LocalGuardedPreparation(w)
                    if w.obj_id() == old.obj_id() && (w.0.value.payload.is_some()
                        || w.0.value.phase == GuardedPreparationPhase::BlockedRetiredCommand)),
                    "invalid proof attempted terminal replacement or FULL write"
                );
                if call.order >= hit {
                    ensure!(
                        singleton_proof::not_old_output(old, event)?,
                        "invalid proof attempted exact old publication"
                    );
                }
            }
        }
    }
    {
        let writes = h.repo.writes.lock().unwrap();
        for write in writes
            .iter()
            .filter(|w| w.forward > start && w.forward < returned)
        {
            ensure!(
                !matches!(&write.event, GenericKvLogEvent::DeviceTransportNoop(_)),
                "invalid proof forwarded local Noop write"
            );
            if matches!(&write.event, GenericKvLogEvent::DeviceLog(_)) {
                ensure!(
                    write.forward < hit
                        && (write.event == r.clone().to_generic()
                            || write.event == u.clone().to_generic()),
                    "invalid proof forwarded new or resubmitted local command"
                );
            }
            if write.forward >= hit {
                ensure!(
                    singleton_proof::not_old_output(old, &write.event)?,
                    "invalid proof forwarded exact old output/FULL"
                );
            }
        }
    }
    {
        let calls = h.wire.calls.lock().unwrap();
        for call in calls
            .iter()
            .filter(|c| c.order > start && c.order < returned)
        {
            ensure!(
                !matches!(&call.request, SyncRequest::Write(w)
                if matches!(&**w, WriteSyncRequest::DeviceTransportNoop(_))),
                "invalid proof sent Noop"
            );
            if let Some(event) = proof::signed_event(&call.request) {
                if matches!(&event, GenericKvLogEvent::DeviceLog(_)) {
                    ensure!(
                        call.order < hit
                            && (event == r.clone().to_generic() || event == u.clone().to_generic()),
                        "invalid proof uploaded new or resubmitted command"
                    );
                }
                if call.order >= hit {
                    ensure!(
                        singleton_proof::not_old_output(old, &event)?,
                        "invalid proof uploaded exact old output"
                    );
                }
            }
        }
    }
    ensure!(
        h.case.observations.fault_hits.load(Ordering::SeqCst) == 0
            && h.local.hits.load(Ordering::SeqCst) == 0
            && h.wire.noops.lock().unwrap().is_empty(),
        "selected proof control accidentally used transport fault/Noop"
    );
    Ok(())
}

pub(super) fn no_resubmit(h: &harness::Harness, hit: usize, end: usize) -> Result<()> {
    let command = |event: &GenericKvLogEvent| {
        matches!(
            event,
            GenericKvLogEvent::DeviceLog(_) | GenericKvLogEvent::DeviceTransportNoop(_)
        )
    };
    ensure!(
        h.repo
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.order >= hit && c.order < end)
            .all(|c| !matches!(&c.op, repo::Op::Write(_, e) if command(e))),
        "continuation attempted a DeviceLog command or Noop"
    );
    ensure!(
        h.repo
            .writes
            .lock()
            .unwrap()
            .iter()
            .filter(|w| w.forward >= hit && w.forward < end)
            .all(|w| !command(&w.event)),
        "continuation forwarded a DeviceLog command or Noop"
    );
    ensure!(
        h.wire
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.order >= hit && c.order < end)
            .all(|c| !matches!(&c.request, SyncRequest::Write(w)
            if matches!(&**w, WriteSyncRequest::DeviceTransportNoop(_)))
                && proof::signed_event(&c.request).is_none_or(|event| !command(&event))),
        "continuation uploaded a DeviceLog command or Noop"
    );
    Ok(())
}

pub(super) fn same_receipt(
    value: &Receipt,
    receipt: &Receipt,
    source: &ProducerSourceRef,
    uuid: &str,
) -> bool {
    value.request_stage.source == receipt.request_stage.source
        && matches!(&value.decision_stage, Some(ProducerDecisionStage::Attempted { source: actual, observation })
            if actual == source && *observation == ProducerSourceObservation::RawAcknowledged)
        && value.request_stage.observation == ProducerSourceObservation::RawAcknowledged
        && value.request_id.as_deref() == Some(uuid)
        && value.phase == ProducerPhase::Queued
        && value.canonical_state == ProducerCanonicalState::Applied
        && value.material_stage == Some(ProducerMaterialStage::Ready)
        && ["ready", "done", "settled"].contains(&value.readiness.as_str())
}
