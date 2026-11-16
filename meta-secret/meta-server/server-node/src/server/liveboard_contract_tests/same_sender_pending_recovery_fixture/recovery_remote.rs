use super::*;

impl Scenario {
    pub async fn claim(&self, exact: &str) -> Result<SsClaim> {
        let log = PersistentSharedSecret::from(self.canonical.clone())
            .get_ss_log_obj(self.a.creds.vault_name.clone())
            .await?;
        let claim = log
            .claims
            .values()
            .find(|claim| claim.id.wasm_id_str() == exact)
            .context("exact canonical Recover")?;
        assert!(
            !log.liveboard_recovery_results.contains_key(&claim.id),
            "membership retirement must not invent a response winner"
        );
        Ok(claim.clone())
    }
    pub async fn assert_claim(
        &self,
        original: &SsClaim,
        expected: SsDistributionStatus,
    ) -> Result<()> {
        let claim = self.claim(&original.id.wasm_id_str()).await?;
        assert!(claim.id == original.id && claim.dist_claim_id == original.dist_claim_id);
        assert!(
            claim.sender == *self.a.creds.device_id() && claim.dist_claim_id.pass_id == self.pass
        );
        assert_eq!(claim.receivers.len(), 2);
        assert!(
            claim.receivers.contains(self.b.creds.device_id())
                && claim.receivers.contains(self.c.creds.device_id())
        );
        assert!(
            claim
                .receivers
                .iter()
                .all(|id| claim.status.get(id) == Some(&expected))
        );
        self.wire.assert_no_recovery_decisions();
        Ok(())
    }
    pub async fn assert_admitted_recovery(&self, exact: &str) -> Result<SsClaim> {
        let claim = self.claim(exact).await?;
        let source: SsDeviceLogObject = PersistentSharedSecret::from(self.a.service.p_obj.clone())
            .find_ss_device_log_tail_event(self.a.creds.device_id().clone())
            .await?
            .context("actual A stored Recover source")?;
        assert!(source.0.value.id == claim.id && source.0.value.dist_claim_id.pass_id == self.pass);
        let raw = self
            .canonical
            .repo
            .find_one(source.obj_id())
            .await?
            .context("Recover must reach server raw BEFORE Remove")?;
        assert!(raw == source.clone().to_generic());
        let actions = self.wire.actions();
        let signed = actions.iter().find(|action| matches!(&action.payload,
            SignedActionPayload::Event(GenericKvLogEvent::SsDeviceLog(e)) if e.obj_id() == source.obj_id()
        )).context("actual signed A Recover intake")?;
        assert_eq!(signed.signer, *self.a.creds.device_id());
        signed.verify(&self.a.creds.device().keys.dsa_pk)?;
        self.assert_claim(&claim, SsDistributionStatus::Pending)
            .await?;
        self.b.member().await?;
        self.c.member().await?;
        Ok(claim)
    }
    pub async fn assert_pending_remove(
        &self,
        receipt: &LiveboardProducerReceipt,
    ) -> Result<String> {
        assert_eq!(receipt.kind, ProducerKind::RemoveDevice);
        let id = receipt
            .request_id
            .clone()
            .context("actual server-issued Remove UUID")?;
        assert_eq!(
            receipt.target_device_id.as_deref(),
            Some(self.c.creds.device_id().to_string().as_str())
        );
        let raw = self
            .canonical
            .repo
            .find_one(receipt.request_stage.source.event_id.clone())
            .await?
            .context("same A Remove R admitted after Recover")?
            .device_log()?;
        assert_eq!(
            producer_source_commitment(&raw)?,
            receipt.request_stage.source.body_commitment
        );
        let VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(request)) =
            &raw.0.value
        else {
            anyhow::bail!("actual source must be a Remove request")
        };
        assert!(request.sender.user_data.device.device_id == *self.a.creds.device_id());
        assert!(request.target.user_data.device.device_id == *self.c.creds.device_id());
        let p = PersistentVault::from(self.canonical.clone());
        let log = p
            .get_vault_log_artifact(self.a.creds.vault_name.clone())
            .await?;
        let binding = log
            .0
            .value
            .liveboard_removal_pending
            .iter()
            .find(|p| p.request_id == id)
            .context("canonical exact pending binding")?;
        assert!(binding.request == *request);
        assert_eq!(
            p.get_vault(self.a.creds.vault_name.clone())
                .await?
                .0
                .value
                .members()
                .len(),
            3
        );
        let info = self.b.member().await?;
        assert!(
            info.liveboard_state
                .context("actual B metadata")?
                .operations
                .iter()
                .any(|op| op.kind == LiveboardOperationKind::RemoveDevice
                    && op.request_id == id
                    && op.can_approve),
            "B eligibility must come from refreshed Core state"
        );
        Ok(id)
    }
    pub async fn approve(
        &self,
        id: &str,
        request_receipt: &LiveboardProducerReceipt,
    ) -> Result<()> {
        let decision = self
            .b
            .service
            .decide_remove_request(CanonicalDecisionInput {
                request_id: id.into(),
                accepted: true,
            })
            .await?;
        assert_eq!(decision.request_id.as_deref(), Some(id));
        assert!(decision.error_code.is_none());
        let actions = self.wire.actions();
        let action = actions
            .iter()
            .find(|action| {
                matches!(&action.payload,
                    SignedActionPayload::GuardedRemovalDecision(g) if g.request_id == id
                )
            })
            .context("actual B guarded decision, no hand-built authority")?;
        assert_eq!(action.signer, *self.b.creds.device_id());
        action.verify(&self.b.creds.device().keys.dsa_pk)?;
        let SignedActionPayload::GuardedRemovalDecision(guard) = &action.payload else {
            unreachable!()
        };
        let VaultActionEvent::Update(VaultActionUpdateEvent::RemoveMember(update)) =
            &guard.event.0.value
        else {
            anyhow::bail!("guarded Remove body missing")
        };
        let original = self
            .canonical
            .repo
            .find_one(request_receipt.request_stage.source.event_id.clone())
            .await?
            .context("exact original Remove R")?
            .device_log()?;
        assert!(
            original.0.value
                == VaultActionEvent::Request(VaultActionRequestEvent::RemoveMember(
                    update.request.clone()
                ))
        );
        assert!(update.request.sender.user_data.device.device_id == *self.a.creds.device_id());
        assert!(update.target.user_data.device.device_id == *self.c.creds.device_id());
        let log = PersistentVault::from(self.canonical.clone())
            .get_vault_log_artifact(self.a.creds.vault_name.clone())
            .await?;
        let result = log
            .0
            .value
            .liveboard_removal_results
            .iter()
            .find(|r| r.request_id == id)
            .context("canonical exact removal result")?;
        assert_eq!(result.responder_id, *self.b.creds.device_id());
        assert_eq!(result.requester_id, *self.a.creds.device_id());
        assert_eq!(result.target_id, *self.c.creds.device_id());
        assert_eq!(serde_json::to_value(result)?["outcome"], "accepted");
        assert_eq!(serde_json::to_value(result)?["applicationState"], "applied");
        let digest = meta_secret_core::crypto::utils::sha256_hex(&String::from_utf8(
            canonical_json(action)?,
        )?);
        assert!(
            result
                .source
                .as_ref()
                .is_some_and(|s| s.event_id == guard.event.obj_id()
                    && s.signer == action.signer
                    && s.signed_action_hash == digest)
        );
        Ok(())
    }
    pub async fn assert_after_remote_remove(
        &self,
        claim: &SsClaim,
        before_approval: usize,
    ) -> Result<()> {
        let vault = PersistentVault::from(self.canonical.clone())
            .get_vault(self.a.creds.vault_name.clone())
            .await?
            .to_data();
        assert_eq!(vault.members().len(), 2);
        assert!(
            vault.is_member(self.a.creds.device_id()) && vault.is_member(self.b.creds.device_id())
        );
        assert!(
            matches!(vault.find_user(self.c.creds.device_id()), Some(UserMembership::Outsider(o)) if o.status == UserDataOutsiderStatus::Declined)
        );
        self.assert_claim(claim, SsDistributionStatus::Declined)
            .await?;
        for (role, actor, expected_status) in [
            ("A requester", &self.a, "declined"),
            ("B receiver", &self.b, "unknown"),
        ] {
            let info = actor.member().await?;
            assert_eq!(info.member.vault.members().len(), 2);
            let state = info
                .liveboard_state
                .context(format!("{role}: current remaining member metadata"))?;
            let op = state
                .operations
                .iter()
                .find(|op| {
                    op.kind == LiveboardOperationKind::Recover
                        && op.request_id == claim.id.wasm_id_str()
                })
                .context(format!("{role}: exact retired Recover metadata"))?;
            assert_eq!(
                op.requester_id,
                self.a.creds.device_id().to_string(),
                "{role}: original requester"
            );
            assert_eq!(
                op.secret_id.as_deref(),
                Some(self.pass.id_str().as_str()),
                "{role}: exact Pass"
            );
            assert_eq!(
                op.client_status, expected_status,
                "{role}: retired client status"
            );
            assert!(
                !op.can_approve && !op.can_decline,
                "{role}: retired Recover cannot be decided"
            );
            assert!(
                op.eligible_peer_ids.is_empty(),
                "{role}: retired Recover has no eligible peers"
            );
            assert!(
                op.responder_id.is_none() && op.outcome.is_none(),
                "{role}: membership retirement has no response winner"
            );
            self.assert_full_copy(actor).await?;
        }
        for action in self.wire.actions().iter().skip(before_approval) {
            if let SignedActionPayload::Event(GenericKvLogEvent::SsWorkflow(
                SsWorkflowObject::Distribution(event),
            )) = &action.payload
            {
                assert!(
                    event
                        .value
                        .secret_message
                        .cipher_text()
                        .channel
                        .receiver()
                        .to_device_id()
                        != *self.c.creds.device_id(),
                    "removed C must receive no new material"
                );
            }
        }
        Ok(())
    }
}
