use super::*;

pub(super) async fn snapshot(h: &fixture::Harness) -> HashMap<ArtifactId, GenericKvLogEvent> {
    h.world
        .repo
        .inner
        .get_db()
        .await
        .into_iter()
        .filter(|(_, e)| {
            matches!(
                e,
                GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_))
                    | GenericKvLogEvent::SsDeviceLog(_)
                    | GenericKvLogEvent::SsLog(_)
            )
        })
        .collect()
}

pub(super) async fn packets(h: &fixture::Harness, three: bool) -> Result<Vec<SsWorkflowObject>> {
    let actors = [
        &h.world.a.user_creds,
        &h.world.b.user_creds,
        &h.world.c.user_creds,
    ];
    let mut result = vec![];
    let mut shares = vec![];
    let mut generation = None;
    for creds in actors.into_iter().take(if three { 3 } else { 2 }) {
        let event = h
            .world
            .repo
            .inner
            .find_one(ArtifactId::from(descriptor(&h.pass, creds.device_id())))
            .await?
            .context("actual current survivor output")?
            .shared_secret()?;
        let data = event.clone().to_distribution_data()?;
        ensure!(
            data.vault_name == h.world.a.user_creds.vault_name
                && data.claim_id.pass_id == h.pass
                && data.secret_message.cipher_text().channel.sender()
                    == &h.world.a.user_creds.device_creds.device.keys.transport_pk
                && data.secret_message.cipher_text().channel.receiver()
                    == &creds.device_creds.device.keys.transport_pk,
            "published output lost exact owner/recipient/pass/Vault binding"
        );
        if let Some(generation) = &generation {
            ensure!(&data.claim_id == generation, "survivor generations differ");
        }
        generation = Some(data.claim_id.clone());
        let plain = data
            .secret_message
            .cipher_text()
            .decrypt(&creds.device_creds.secret_box.transport.sk)?;
        let share = UserShareDto::try_from(&plain.msg)?;
        ensure!(
            share.share_blocks[0].config.threshold == if three { 2 } else { 1 },
            "actual output has wrong threshold"
        );
        shares.push(share);
        result.push(event);
    }
    if three {
        for (left, right) in [(0, 1), (0, 2), (1, 2)] {
            ensure!(
                recover_from_shares(vec![shares[left].clone(), shares[right].clone()])?.text
                    == SECRET,
                "actual K2 pair does not recover original secret"
            );
        }
    } else {
        for share in shares {
            ensure!(
                recover_from_shares(vec![share])?.text == SECRET,
                "actual K1 copy does not recover original secret"
            );
        }
    }
    Ok(result)
}

pub(super) async fn publication(h: &fixture::Harness) -> Result<()> {
    let outputs = packets(h, false).await?;
    let update = h.update().await?;
    let expected = reshare_generation_claim_id(&h.original.id, &serde_json::to_string(&update)?);
    ensure!(
        outputs.iter().all(|p| p
            .clone()
            .to_distribution_data()
            .is_ok_and(|d| d.claim_id.id == expected)),
        "publication did not use exact oldG/exact Accepted update-derived Gprime"
    );
    let ops = h.world.repo.operations.lock().unwrap().clone();
    let delete = ops
        .iter()
        .position(|op| matches!(op,repo::Operation::TryDelete(key) if *key==h.old_target.obj_id()));
    ensure!(
        !ops.iter()
            .any(|op| matches!(op,repo::Operation::Delete(key) if *key==h.old_target.obj_id())),
        "target cleanup used infallible deletion"
    );
    let writes: Vec<_> = ops
        .iter()
        .enumerate()
        .filter_map(|(order, op)| match op {
            repo::Operation::Write(e) => Some((order, e.clone())),
            _ => None,
        })
        .collect();
    let mut intended: Vec<_> = outputs
        .into_iter()
        .map(ToGenericEvent::to_generic)
        .collect();
    let output_attempts: Vec<_> = writes
        .iter()
        .filter_map(|(_, event)| match event {
            GenericKvLogEvent::SsWorkflow(SsWorkflowObject::Distribution(_)) => Some(event),
            _ => None,
        })
        .collect();
    ensure!(
        output_attempts.len() == 2 && output_attempts.iter().all(|event| intended.contains(event)),
        "initial publication emitted outputs beyond the exact survivor pair"
    );
    let own_claim: Vec<_> = writes
        .iter()
        .filter_map(|(_, e)| match e {
            GenericKvLogEvent::SsDeviceLog(c) if c.0.value.id == expected => Some(e.clone()),
            _ => None,
        })
        .collect();
    let aggregate: Vec<_> = writes
        .iter()
        .filter_map(|(_, e)| match e {
            GenericKvLogEvent::SsLog(c) if c.0.value.claims.contains_key(&expected) => {
                Some(e.clone())
            }
            _ => None,
        })
        .collect();
    ensure!(
        own_claim.len() == 1 && aggregate.len() == 1,
        "publication omitted exact new claim metadata"
    );
    let GenericKvLogEvent::SsDeviceLog(own) = &own_claim[0] else {
        unreachable!()
    };
    let GenericKvLogEvent::SsLog(log) = &aggregate[0] else {
        unreachable!()
    };
    let claim = &own.0.value;
    ensure!(
        claim.sender == *h.world.a.user_creds.device_id()
            && claim.distribution_type == SecretDistributionType::Split
            && claim.vault_name == h.world.a.user_creds.vault_name
            && claim.dist_claim_id.id == expected
            && claim.dist_claim_id.pass_id == h.pass
            && claim.receivers == vec![h.world.b.user_creds.device_id().clone()]
            && claim.status.statuses.len() == 1
            && claim.status.statuses.get(h.world.b.user_creds.device_id())
                == Some(&SsDistributionStatus::Pending)
            && log.0.value.claims.get(&expected) == Some(claim)
            && intended.iter().all(|event| event
                .clone()
                .shared_secret()
                .and_then(SsWorkflowObject::to_distribution_data)
                .is_ok_and(|data| data.claim_id == claim.dist_claim_id)),
        "actual survivor outputs and both claim records lack the exact complete Gprime binding"
    );
    intended.extend(own_claim);
    intended.extend(aggregate);
    let delete = delete
        .context("missing fallible exact-old-target cleanup after real survivor publication")?;
    let mut first_write = usize::MAX;
    let mut last_write = 0;
    for event in intended {
        let order = writes
            .iter()
            .find_map(|(o, e)| (e == &event).then_some(*o))
            .context("actual expected survivor/claim write missing")?;
        first_write = first_write.min(order);
        last_write = last_write.max(order);
        ensure!(
            h.world.repo.inner.find_one(event.obj_id()).await? == Some(event.clone()),
            "actual publication bytes differ from intended record"
        );
        ensure!(
            ops.iter().enumerate().any(|(o, op)| o > order
                && o < delete
                && matches!(op,
            repo::Operation::Read(key,Some(actual)) if *key==event.obj_id() && actual==&event)),
            "target deletion preceded exact OUTER production survivor/claim readback"
        );
    }
    let old = h.old_target.clone().to_generic();
    ensure!(ops.iter().enumerate().any(|(o,op)|o<first_write && matches!(op,repo::Operation::Read(key,Some(actual)) if *key==old.obj_id() && actual==&old)),"initial target candidate was not captured before publication");
    ensure!(ops.iter().enumerate().any(|(o,op)|o>last_write && o<delete && matches!(op,repo::Operation::Read(key,Some(actual)) if *key==old.obj_id() && actual==&old)),"exact captured old target was not reread before fallible delete");
    Ok(())
}

pub(super) async fn target_absent(h: &fixture::Harness) -> Result<()> {
    ensure!(
        h.world
            .repo
            .inner
            .find_one(h.old_target.obj_id())
            .await?
            .is_none(),
        "obsolete actual owner-to-target packet retained after completed legacy removal"
    );
    let ops = h.world.repo.operations.lock().unwrap();
    let delete = ops
        .iter()
        .rposition(|op| matches!(op,repo::Operation::TryDelete(key) if *key==h.old_target.obj_id()))
        .context("actual target absence lacks fallible production deletion")?;
    ensure!(
        ops.iter().enumerate().any(|(order, op)| order > delete
            && matches!(op,repo::Operation::Read(key,None) if *key==h.old_target.obj_id())),
        "actual target deletion lacks OUTER production absence readback"
    );
    Ok(())
}
