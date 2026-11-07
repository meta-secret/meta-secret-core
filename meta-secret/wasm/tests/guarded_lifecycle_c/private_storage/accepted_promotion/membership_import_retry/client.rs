use super::*;

pub(super) struct Client<R: KvLogEventRepo> {
    pub p: Arc<PersistentObject<R>>,
    pub creds: UserCreds,
    pub master: TransportSk,
    pub wire: Arc<wire::Forward<R>>,
}
impl<R: KvLogEventRepo> Client<R> {
    pub async fn new(repo: Arc<R>, creds: UserCreds, remote: Remote) -> Result<Self> {
        let client = Self::open(repo, creds, remote, TransportDsaKeyPair::generate().sk());
        PersistentCredentials {
            p_obj: client.p.clone(),
            master_key: client.master.clone(),
        }
        .save_user_creds(client.creds.clone())
        .await?;
        Ok(client)
    }
    pub fn open(repo: Arc<R>, creds: UserCreds, remote: Remote, master: TransportSk) -> Self {
        Self {
            wire: Arc::new(wire::Forward::new(remote, repo.clone(), creds.user())),
            p: Arc::new(PersistentObject::new(repo)),
            creds,
            master,
        }
    }
    pub async fn sync(&self) -> Result<()> {
        SyncGateway::new(
            "actual-membership-import-retry".into(),
            self.p.clone(),
            self.wire.clone(),
            self.master.clone(),
        )
        .sync(self.creds.user())
        .await
    }
    pub async fn pull_vault(&self) -> Result<()> {
        let tail = PersistentVault::from(self.p.clone())
            .vault_tail(self.creds.user())
            .await?;
        let data = self
            .wire
            .send(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
                VaultRequest {
                    sender: self.creds.user(),
                    tail,
                },
            ))))
            .await?
            .to_data()?;
        for event in data.0 {
            self.p.repo.save(event).await?;
        }
        Ok(())
    }
    pub async fn pull_ss(&self) -> Result<Vec<GenericKvLogEvent>> {
        let ss_log = self
            .p
            .find_free_id_by_obj_desc(SsLogDescriptor::from(self.creds.vault_name.clone()))
            .await?;
        let data = self
            .wire
            .send(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
                SsRequest {
                    sender: self.creds.user(),
                    ss_log,
                },
            ))))
            .await?
            .to_data()?;
        for event in &data.0 {
            if self.p.repo.find_one(event.obj_id()).await?.is_some() {
                self.p.repo.replace(event.clone()).await?;
            } else {
                self.p.repo.save(event.clone()).await?;
            }
        }
        Ok(data.0)
    }
    pub async fn send_latest(
        &self,
        server_repo: &Arc<InMemKvLogEventRepo>,
    ) -> Result<DeviceLogObject> {
        let body: DeviceLogObject = self
            .p
            .find_tail_event(DeviceLogDescriptor::from(self.creds.user_id()))
            .await?
            .context("actual production-generated body")?;
        let action = sign_event(
            body.clone().to_generic(),
            self.creds.device_id().clone(),
            &self.creds.device_creds.key_manager()?.dsa,
        )?;
        self.wire
            .send(SyncRequest::Write(Box::new(WriteSyncRequest::Event(
                action,
            ))))
            .await?
            .ensure_success()?;
        ensure!(
            server_repo.find_one(body.obj_id()).await? == Some(body.clone().to_generic()),
            "actual signed producer body was not durably admitted"
        );
        Ok(body)
    }
}
