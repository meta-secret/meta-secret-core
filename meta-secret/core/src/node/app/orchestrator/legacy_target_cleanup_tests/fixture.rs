use super::*;

pub(super) struct World {
    pub a: MetaOrchestrator<repo::Observed>,
    pub b: MetaOrchestrator<InMemKvLogEventRepo>,
    pub c: MetaOrchestrator<InMemKvLogEventRepo>,
    pub server: ServerVaultAction<InMemKvLogEventRepo>,
    pub repo: Arc<repo::Observed>,
}
pub(super) struct Harness {
    pub world: World,
    pub pass: MetaPasswordId,
    pub original: SsClaim,
    pub old_target: SsWorkflowObject,
    pub request: RemoveMemberRequestEvent,
    pub request_id: String,
}

impl World {
    async fn new() -> Result<Self> {
        let f = FixtureRegistry::empty();
        let repo = Arc::new(repo::Observed::new(f.state.p_obj.client.repo.clone()));
        let original = &f.state.p_obj.client;
        let p = Arc::new(PersistentObject {
            repo: repo.clone(),
            device_log_append_guard: original.device_log_append_guard.clone(),
            client_operation_guard: original.client_operation_guard.clone(),
            device_sync_guard: original.device_sync_guard.clone(),
        });
        ensure!(
            Arc::ptr_eq(
                &p.device_log_append_guard,
                &original.device_log_append_guard
            ) && Arc::ptr_eq(&p.client_operation_guard, &original.client_operation_guard)
                && Arc::ptr_eq(&p.device_sync_guard, &original.device_sync_guard),
            "observer changed actual local ownership guards"
        );
        ensure!(
            !Arc::ptr_eq(&repo.inner, &f.state.p_obj.client_b.repo)
                && !Arc::ptr_eq(&repo.inner, &f.state.p_obj.vd.repo)
                && !Arc::ptr_eq(&repo.inner, &f.state.p_obj.server.repo)
                && !Arc::ptr_eq(&f.state.p_obj.client_b.repo, &f.state.p_obj.vd.repo),
            "legacy owner/approver proof requires distinct actual repositories"
        );
        let world = Self {
            a: MetaOrchestrator {
                p_obj: p,
                user_creds: f.state.user_creds.client.clone(),
            },
            b: MetaOrchestrator {
                p_obj: f.state.p_obj.client_b.clone(),
                user_creds: f.state.user_creds.client_b.clone(),
            },
            c: MetaOrchestrator {
                p_obj: f.state.p_obj.vd.clone(),
                user_creds: f.state.user_creds.vd.clone(),
            },
            server: ServerVaultAction {
                p_obj: f.state.p_obj.server.clone(),
                server_device: f.state.device_creds.server.device.clone(),
            },
            repo,
        };
        world
            .server
            .do_processing(VaultActionEvent::Init(VaultActionInitEvent::CreateVault(
                CreateVaultEvent::from(UserDataMember::from(world.a.user_creds.user())),
            )))
            .await?;
        world.import_all().await?;
        world.join(world.b.user_creds.user()).await?;
        world.join(world.c.user_creds.user()).await?;
        ensure!(
            world.vault().await?.members().len() == 3,
            "real domain bootstrap did not create three members"
        );
        Ok(world)
    }

    pub async fn vault(&self) -> Result<VaultData> {
        Ok(PersistentVault::from(self.server.p_obj.clone())
            .get_vault(self.a.user_creds.vault_name.clone())
            .await?
            .to_data())
    }
    pub async fn import_all(&self) -> Result<()> {
        // Normal authorized server reads reconcile these status records.
        let p_vault = PersistentVault::from(self.server.p_obj.clone());
        for actor in [&self.a.user_creds, &self.b.user_creds, &self.c.user_creds] {
            p_vault
                .update_vault_membership_info_for_user(actor.user())
                .await?;
        }
        let events = self.server.p_obj.repo.get_db().await;
        for event in events.values().filter(|e| {
            matches!(
                e,
                GenericKvLogEvent::Vault(_)
                    | GenericKvLogEvent::VaultLog(_)
                    | GenericKvLogEvent::VaultStatus(_)
            )
        }) {
            self.a.p_obj.repo.save(event.clone()).await?;
            self.b.p_obj.repo.save(event.clone()).await?;
            self.c.p_obj.repo.save(event.clone()).await?;
        }
        Ok(())
    }
    pub async fn last_action<R: KvLogEventRepo>(
        actor: &MetaOrchestrator<R>,
    ) -> Result<VaultActionEvent> {
        Ok(actor
            .p_obj
            .find_tail_event(DeviceLogDescriptor::from(actor.user_creds.user_id()))
            .await?
            .context("actual emitted DeviceLog action")?
            .0
            .value)
    }
    pub async fn join(
        &self,
        candidate: crate::node::common::model::user::common::UserData,
    ) -> Result<JoinClusterEvent> {
        let request = JoinClusterEvent::from(candidate);
        self.server
            .do_processing(VaultActionEvent::Request(
                VaultActionRequestEvent::JoinCluster(request.clone()),
            ))
            .await?;
        self.import_all().await?;
        self.a
            .update_membership(request.clone(), JoinActionUpdate::Accept)
            .await?;
        let action = Self::last_action(&self.a).await?;
        ensure!(
            matches!(&action, VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(u)) if u.request == request && u.sender.user_data == self.a.user_creds.user()),
            "Join must process the actually emitted A update"
        );
        self.server.do_processing(action).await?;
        self.import_all().await?;
        Ok(request)
    }
}

impl Harness {
    pub async fn new() -> Result<Self> {
        let world = World::new().await?;
        let vault = world.vault().await?;
        let pass = SecurePassInfo::from(PlainPassInfo::new(
            "legacy-owner-approver".into(),
            SECRET.into(),
        ));
        let pass_id = pass.pass_id.clone();
        MetaDistributor {
            p_obj: world.a.p_obj.clone(),
            user_creds: Arc::new(world.a.user_creds.clone()),
            vault_member: VaultMember {
                member: UserDataMember::from(world.a.user_creds.user()),
                vault: vault.clone(),
            },
        }
        .distribute(
            VaultMember {
                member: UserDataMember::from(world.a.user_creds.user()),
                vault,
            },
            pass,
        )
        .await?;
        let add_pass = World::last_action(&world.a).await?;
        ensure!(
            matches!(&add_pass, VaultActionEvent::Request(VaultActionRequestEvent::AddMetaPass(a)) if a.meta_pass_id == pass_id),
            "real distributor did not queue AddMetaPass"
        );
        world.server.do_processing(add_pass).await?;
        world.import_all().await?;
        let a_ss = PersistentSharedSecret::from(world.a.p_obj.clone());
        let original = a_ss
            .find_ss_device_log_tail_event(world.a.user_creds.device_id().clone())
            .await?
            .context("actual initial Split")?
            .to_distribution_request();
        a_ss.save_ss_log_event(original.clone()).await?;
        PersistentSharedSecret::from(world.b.p_obj.clone())
            .save_ss_log_event(original.clone())
            .await?;
        let b_packet = packet(&world.a.p_obj, &pass_id, world.b.user_creds.device_id()).await?;
        world.b.p_obj.repo.save(b_packet).await?;
        let old_target = packet(&world.a.p_obj, &pass_id, world.c.user_creds.device_id()).await?;
        let target_data = old_target.clone().to_distribution_data()?;
        ensure!(
            original.sender == *world.a.user_creds.device_id()
                && target_data.claim_id == original.dist_claim_id
                && target_data.vault_name == world.a.user_creds.vault_name
                && target_data.secret_message.cipher_text().channel.sender()
                    == &world.a.user_creds.device_creds.device.keys.transport_pk
                && target_data.secret_message.cipher_text().channel.receiver()
                    == &world.c.user_creds.device_creds.device.keys.transport_pk
                && original.status.statuses.get(world.c.user_creds.device_id())
                    == Some(&SsDistributionStatus::Pending),
            "setup must retain A-owned, unacknowledged C packet from actual original G"
        );
        let request = world
            .b
            .request_remove_member(UserDataMember::from(world.c.user_creds.user()))
            .await?;
        let actual_request = World::last_action(&world.b).await?;
        ensure!(
            matches!(&actual_request,VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(r)) if r==&request),
            "B did not emit exact actual removal request"
        );
        ensure!(
            request.share_transfers.len() == 1
                && request.share_transfers[0].receiver == *world.a.user_creds.device_id()
                && request.share_transfers[0].claim_id.as_ref() == Some(&original.id),
            "real B transfer is not bound to oldG/A approver"
        );
        world.server.do_processing(actual_request).await?;
        world.import_all().await?;
        let log = PersistentVault::from(world.server.p_obj.clone())
            .get_vault_log_artifact(world.a.user_creds.vault_name.clone())
            .await?;
        let request_id = log
            .0
            .value
            .liveboard_removal_pending
            .iter()
            .find(|b| b.request == request)
            .context("actual server Pending removal binding")?
            .request_id
            .clone();
        world.repo.reset();
        Ok(Self {
            world,
            pass: pass_id,
            original,
            old_target,
            request,
            request_id,
        })
    }

    pub async fn approve(&self) -> Result<()> {
        self.world
            .a
            .accept_remove_member(self.world.c.user_creds.device_id().to_string())
            .await
    }
    pub async fn update(&self) -> Result<RemoveMemberEvent> {
        match World::last_action(&self.world.a).await? {
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(u)) => {
                ensure!(
                    u.request == self.request
                        && u.sender.user_data == self.world.a.user_creds.user()
                        && u.decision == RemoveMemberDecision::Accepted,
                    "local legacy approval roles/update differ"
                );
                Ok(u)
            }
            _ => anyhow::bail!("real A Accepted update missing"),
        }
    }
    pub async fn apply_actual_update(&self) -> Result<bool> {
        let update = self.update().await?;
        self.world
            .server
            .do_processing(VaultActionEvent::Update(
                VaultActionUpdateEvent::RemoveMember(update.clone()),
            ))
            .await?;
        let vault = self.world.vault().await?;
        let log = PersistentVault::from(self.world.server.p_obj.clone())
            .get_vault_log_artifact(self.world.a.user_creds.vault_name.clone())
            .await?;
        ensure!(
            vault.members().len() == 2
                && !vault.is_member(self.world.c.user_creds.device_id())
                && log
                    .0
                    .value
                    .liveboard_removal_results
                    .iter()
                    .any(|r| r.request_id == self.request_id
                        && r.application_state == RemovalApplicationState::Applied),
            "actual domain processing did not persist canonical2/Applied"
        );
        let retained = log
            .0
            .value
            .updates
            .contains(&VaultActionUpdateEvent::RemoveMember(update));
        self.world.import_all().await?;
        Ok(retained)
    }
    pub async fn different_generation_target(&self) -> Result<SsWorkflowObject> {
        let p = Arc::new(PersistentObject::in_mem());
        let vault = self.world.vault().await?;
        let member = VaultMember {
            member: UserDataMember::from(self.world.a.user_creds.user()),
            vault,
        };
        MetaDistributor {
            p_obj: p.clone(),
            user_creds: Arc::new(self.world.a.user_creds.clone()),
            vault_member: member.clone(),
        }
        .distribute(
            member,
            SecurePassInfo::from(PlainPassInfo {
                pass_id: self.pass.clone(),
                pass: SECRET.into(),
            }),
        )
        .await?;
        let different = packet(&p, &self.pass, self.world.c.user_creds.device_id()).await?;
        ensure!(
            different.clone().to_distribution_data()?.claim_id.id != self.original.id,
            "fresh genuine distribution did not create distinct generation"
        );
        Ok(different)
    }
}
