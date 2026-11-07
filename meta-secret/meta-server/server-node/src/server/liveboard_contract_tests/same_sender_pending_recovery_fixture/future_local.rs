use super::*;

impl Scenario {
    pub async fn assert_future_local_remove(&self, first_id: &str) -> Result<()> {
        let next = self
            .a
            .service
            .remove_member_checked(self.b.creds.device_id().to_string())
            .await?;
        assert_eq!(next.kind, ProducerKind::RemoveDevice);
        assert_eq!(
            next.target_device_id.as_deref(),
            Some(self.b.creds.device_id().to_string().as_str())
        );
        assert!(next.request_id.as_deref().is_some_and(|id| id != first_id));
        let Some(ProducerDecisionStage::Attempted {
            source: actual_u, ..
        }) = &next.decision_stage
        else {
            anyhow::bail!(
                "next local removal must carry its actual U, never a guessed/cleared context"
            )
        };
        let info = self.a.member().await?;
        assert_eq!(info.member.vault.members().len(), 1);
        let state = info.liveboard_state.context("actual singleton metadata")?;
        assert!(
            state
                .secrets
                .iter()
                .any(|s| s.secret_id == self.pass.id_str()
                    && s.primary_action == "showLocal"
                    && s.readiness == "ready")
        );
        let vault = PersistentVault::from(self.canonical.clone())
            .get_vault(self.a.creds.vault_name.clone())
            .await?
            .to_data();
        assert_eq!(vault.members().len(), 1);
        assert!(vault.is_member(self.a.creds.device_id()));
        assert!(
            matches!(vault.find_user(self.b.creds.device_id()), Some(UserMembership::Outsider(o)) if o.status == UserDataOutsiderStatus::Declined)
        );
        let id = next
            .request_id
            .as_deref()
            .context("local removal: actual next UUID")?;
        assert!(
            meta_secret_core::node::api::is_liveboard_request_id(id),
            "local removal: valid next UUID"
        );
        let r = self
            .assert_local_source(
                "local R",
                &next.request_stage.source,
                VerifiedProducerKind::RemoveRequest,
                id,
            )
            .await?;
        let u = self
            .assert_local_source(
                "local U",
                actual_u,
                VerifiedProducerKind::LocalRemoveAccept,
                id,
            )
            .await?;
        assert!(
            u.obj_id() == r.obj_id().next(),
            "local removal: actual full R/U sequence"
        );
        let (
            VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)),
            VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)),
        ) = (&r.0.value, &u.0.value)
        else {
            anyhow::bail!("local removal: ordinary R/U body kinds")
        };
        assert!(
            request.sender == member(&self.a.creds) && request.target == member(&self.b.creds),
            "local R: exact A/B members"
        );
        assert!(
            update.request == *request,
            "local U: complete original Remove request"
        );
        assert!(
            update.sender == request.sender && update.target == request.target,
            "local U: exact A/B roles"
        );
        assert_eq!(
            update.decision,
            RemoveMemberDecision::Accepted,
            "local U: accepted decision"
        );
        assert!(
            matches!(&update.new_status, UserMembership::Outsider(o)
            if o.user_data == request.target.user_data && o.status == UserDataOutsiderStatus::Declined),
            "local U: exact removed B status"
        );
        let log = PersistentVault::from(self.canonical.clone())
            .get_vault_log_artifact(self.a.creds.vault_name.clone())
            .await?;
        let results: Vec<_> = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .filter(|r| r.request_id == id)
            .collect();
        assert_eq!(
            results.len(),
            1,
            "local removal: unique canonical next UUID result"
        );
        let result = results[0];
        assert_eq!(
            result.requester_id,
            *self.a.creds.device_id(),
            "local result: requester A"
        );
        assert_eq!(
            result.target_id,
            *self.b.creds.device_id(),
            "local result: target B"
        );
        assert_eq!(
            result.responder_id,
            *self.a.creds.device_id(),
            "local result: responder A"
        );
        assert_eq!(
            result.outcome,
            DecisionOutcome::Accepted,
            "local result: accepted"
        );
        assert_eq!(
            result.application_state,
            RemovalApplicationState::Applied,
            "local result: applied"
        );
        assert!(
            result.source.is_none(),
            "ordinary local result: no guarded source proof"
        );
        let registry = PersistentRemovalEnvelope {
            p_obj: self.a.service.p_obj.clone(),
        }
        .load(&self.a.creds.user_id())
        .await?;
        assert!(
            registry.is_none_or(|r| r.0.value.producer_context.is_none()),
            "actual current completion must release its own technical producer record"
        );
        self.assert_full_copy(&self.a).await?;
        self.wire.assert_no_recovery_decisions();
        Ok(())
    }
}
