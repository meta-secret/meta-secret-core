use super::*;

/// Distinct local repo; only the actual actor's admitted DeviceLog prefix is copied.
/// Vault/SS state comes from real addressed ServerApp reads, never edited membership.
pub(super) struct Peer {
    pub p: Arc<PersistentObject<InMemKvLogEventRepo>>,
    pub creds: UserCreds,
}
impl Peer {
    pub async fn existing(h: &harness::Harness, creds: UserCreds) -> Result<Self> {
        let p = Arc::new(PersistentObject::new(Arc::new(
            InMemKvLogEventRepo::default(),
        )));
        let stream = ArtifactId::from(DeviceLogDescriptor::from(creds.user_id())).fqdn;
        for event in h.case.h.state.server_repo.get_db().await.into_values() {
            if matches!(&event, GenericKvLogEvent::DeviceLog(_)) && event.obj_id().fqdn == stream {
                p.repo.save(event).await?;
            }
        }
        Ok(Self { p, creds })
    }
    pub async fn pull_vault(&self, h: &harness::Harness) -> Result<()> {
        let tail = PersistentVault::from(self.p.clone())
            .vault_tail(self.creds.user())
            .await?;
        let data = h
            .case
            .h
            .state
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
                VaultRequest {
                    sender: self.creds.user(),
                    tail,
                },
            ))))
            .await?
            .to_data()?;
        self.import(data.0).await
    }
    pub async fn pull_ss(&self, h: &harness::Harness) -> Result<Vec<GenericKvLogEvent>> {
        let ss_log = self
            .p
            .find_free_id_by_obj_desc(SsLogDescriptor::from(self.creds.vault_name.clone()))
            .await?;
        let events = h
            .case
            .h
            .state
            .server
            .handle_client_request(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                SsRequest {
                    sender: self.creds.user(),
                    ss_log,
                },
            ))))
            .await?
            .to_data()?
            .0;
        self.import(events.clone()).await?;
        Ok(events)
    }
    async fn import(&self, events: Vec<GenericKvLogEvent>) -> Result<()> {
        for event in events {
            if self.p.repo.find_one(event.obj_id()).await?.is_some() {
                self.p.repo.replace(event).await?;
            } else {
                self.p.repo.save(event).await?;
            }
        }
        Ok(())
    }
    pub async fn send_latest(&self, h: &harness::Harness) -> Result<DeviceLogObject> {
        let body: DeviceLogObject = self
            .p
            .find_tail_event(DeviceLogDescriptor::from(self.creds.user_id()))
            .await?
            .context("actual peer-produced DeviceLog")?;
        let signed = sign_event(
            body.clone().to_generic(),
            self.creds.device_id().clone(),
            &self.creds.device_creds.key_manager()?.dsa,
        )?;
        h.case
            .h
            .state
            .server
            .handle_client_request(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                signed,
            ))))
            .await?
            .ensure_success()?;
        ensure!(
            h.case.h.state.server_repo.find_one(body.obj_id()).await?
                == Some(body.clone().to_generic()),
            "real signed peer command lacks durable original source"
        );
        Ok(body)
    }
}

pub(super) async fn delete_at_two(h: &harness::Harness, a: &Peer) -> Result<()> {
    a.pull_vault(h).await?;
    SecretDeletionAction {
        p_obj: a.p.clone(),
        user_creds: a.creds.clone(),
    }
    .request(h.case.h.state.passes[0].clone())
    .await?;
    let body = a.send_latest(h).await?;
    let id = match body.0.value {
        VaultActionEvent::Request(VaultActionRequestEvent::DeleteSecret(r)) => r.request_id,
        _ => anyhow::bail!("actual member2 Delete request not produced"),
    };
    a.pull_vault(h).await?;
    let (vault, _) = super::super::canonical(&h.case).await?;
    if !vault
        .0
        .value
        .deleted_secrets
        .contains(&h.case.h.state.passes[0])
    {
        SecretDeletionAction {
            p_obj: a.p.clone(),
            user_creds: a.creds.clone(),
        }
        .decide(&id, true)
        .await?;
        a.send_latest(h).await?;
    }
    let (vault, _) = super::super::canonical(&h.case).await?;
    ensure!(
        vault.0.value.members().len() == 2
            && vault
                .0
                .value
                .deleted_secrets
                .contains(&h.case.h.state.passes[0])
            && vault.0.value.secrets.contains(&h.case.h.state.passes[1]),
        "real member2 Delete did not preserve p1/topology"
    );
    Ok(())
}

pub(super) async fn rejoin_c(h: &harness::Harness) -> Result<(Peer, Peer)> {
    let a = Peer::existing(h, h.case.h.state.a.clone()).await?;
    let c = Peer::existing(h, h.case.h.state.c.clone()).await?;
    PersistentDeviceLog::from(c.p.clone())
        .save_join_request(&c.creds.user())
        .await?;
    let request = PersistentDeviceLog::from(c.p.clone())
        .latest_join_request(&c.creds.user())
        .await?
        .context("actual rejoin UUID")?;
    let request_body = c.send_latest(h).await?;
    ensure!(
        matches!(&request_body.0.value, VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(r)) if r == &request),
        "signed actual C rejoin request differs"
    );
    a.pull_vault(h).await?;
    let prefix = PersistentDeviceLog::from(a.p.clone())
        .find_tail_id(&a.creds.user_id())
        .await?
        .context("actual A prefix")?;
    MetaOrchestrator {
        p_obj: a.p.clone(),
        user_creds: a.creds.clone(),
    }
    .update_membership(request.clone(), JoinActionUpdate::Accept)
    .await?;
    let approved = a.send_latest(h).await?;
    ensure!(
        approved.obj_id() == prefix.next(),
        "real approval broke actual A source sequence"
    );
    let (vault, log) = super::super::canonical(&h.case).await?;
    ensure!(vault.0.value.members().len() == 3 && vault.0.value.is_member(c.creds.device_id())
        && log.0.value.join_results.iter().any(|r| r.request == request
            && r.outcome == meta_secret_core::node::db::events::vault::vault_log_event::JoinResultOutcome::Accepted),
        "real signed rejoin approval did not establish canonical3");
    let local = PersistentVault::from(h.p.clone())
        .get_vault(h.case.h.state.b.vault_name.clone())
        .await?;
    ensure!(
        local.0.value.members().len() == 2,
        "B imported Join before tested sync"
    );
    Ok((a, c))
}
