use super::*;

#[tokio::test]
async fn owner_approver_cleans_own_old_target_before_rejoin() -> Result<()> {
    let h = fixture::Harness::new().await?;
    h.approve().await?;
    let publication = proof::publication(&h).await;
    let absent = proof::target_absent(&h).await;
    let retained = h.apply_actual_update().await?;
    // Continue to the real Join even when the earlier cleanup oracle failed.
    let joined = h.world.join(h.world.c.user_creds.user()).await;
    eprintln!(
        "LEGACY_TARGET owner_approver publication_order_ok={} target_absent={} actual_canonical2_Applied=true actual_canonical_update_retained={retained} real_rejoin_ok={}",
        publication.is_ok(),
        absent.is_ok(),
        joined.is_ok()
    );
    let repeat = if let Ok(join) = joined.as_ref() {
        proof::packets(&h, true).await?;
        let before = proof::snapshot(&h).await;
        h.world.repo.reset();
        h.world
            .a
            .redistribute_existing_secrets_for_join(join)
            .await?;
        ensure!(
            proof::snapshot(&h).await == before && h.world.repo.material_attempts() == 0,
            "stable rejoin repeat regenerated ciphertext or claims"
        );
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "genuine removed-device Join blocked by retained old target generation"
        ))
    };
    publication?;
    absent?;
    joined?;
    repeat
}

#[tokio::test]
async fn wrong_target_generation_is_preserved_before_survivor_publication() -> Result<()> {
    let h = fixture::Harness::new().await?;
    let different = h.different_generation_target().await?;
    // Deliberate negative local corruption uses genuine encrypted producer output.
    h.world.repo.inner.try_delete(different.obj_id()).await?;
    h.world.repo.inner.save(different.clone()).await?;
    let before = proof::snapshot(&h).await;
    h.world.repo.reset();
    let approval = h.approve().await;
    let preserved =
        h.world.repo.inner.find_one(different.obj_id()).await? == Some(different.to_generic());
    let attempts = h.world.repo.material_attempts();
    let unchanged = proof::snapshot(&h).await == before;
    let log = h.world.a.get_ss_log_data().await?;
    let unavailable = h
        .world
        .a
        .audit_join_source(&h.world.vault().await?, &h.pass, &log)
        .await
        .is_err();
    eprintln!(
        "LEGACY_TARGET wrong_generation rejected={} target_preserved={preserved} all_material_claim_write_attempts={attempts} originals_unchanged={unchanged} strict_join_unavailable={unavailable}",
        approval.is_err()
    );
    ensure!(
        approval.is_err() && preserved && attempts == 0 && unchanged && unavailable,
        "different-generation target must reject BEFORE every survivor/claim write and remain untouched"
    );
    Ok(())
}

#[tokio::test]
async fn failed_target_delete_retries_cleanup_with_same_generation_and_zero_new_outputs()
-> Result<()> {
    let h = fixture::Harness::new().await?;
    h.world.repo.arm(h.old_target.obj_id());
    let first = h.approve().await;
    let exact_error = first
        .as_ref()
        .err()
        .is_some_and(|e| e.is::<repo::TargetDeleteBefore>());
    let hits = h.world.repo.fault_hits.load(Ordering::SeqCst);
    let publication = proof::publication(&h).await;
    let target_still_exact = h.world.repo.inner.find_one(h.old_target.obj_id()).await?
        == Some(h.old_target.clone().to_generic());
    let before = proof::snapshot(&h).await;
    let retained = h.apply_actual_update().await?;
    h.world.repo.clear();
    h.world.repo.reset();
    let retry = h.world.a.finalize_pending_removal_resharing().await;
    let attempts = h.world.repo.material_attempts();
    let after = proof::snapshot(&h).await;
    let target_absent = proof::target_absent(&h).await;
    let expected_after: HashMap<_, _> = before
        .into_iter()
        .filter(|(id, _)| id != &h.old_target.obj_id())
        .collect();
    let after_survivors: HashMap<_, _> = after
        .iter()
        .filter(|(id, _)| *id != &h.old_target.obj_id())
        .map(|(id, event)| (id.clone(), event.clone()))
        .collect();
    let same = after_survivors == expected_after;
    eprintln!(
        "LEGACY_TARGET delete_before actual_fault_hits={hits} exact_returned_fault={exact_error} fresh_production_readbacks_before_delete={} target_preserved_at_fault={target_still_exact} actual_canonical2_Applied=true actual_canonical_update_retained={retained} retry_ok={} retry_target_absent={} retry_ALL_material_claim_attempts={attempts} same_Gprime_material={same}",
        publication.is_ok(),
        retry.is_ok(),
        target_absent.is_ok()
    );
    let repeat = h.world.a.finalize_pending_removal_resharing().await;
    ensure!(
        h.world.repo.material_attempts() == 0 && proof::snapshot(&h).await == after,
        "cleanup-only repeat recreated material or claims"
    );
    ensure!(
        hits == 1 && exact_error && publication.is_ok() && target_still_exact,
        "exact fallible target-delete error boundary after fresh publication was not reached"
    );
    retry?;
    repeat?;
    target_absent?;
    ensure!(
        attempts == 0 && same,
        "cleanup retry changed Gprime or generated another output/claim"
    );
    Ok(())
}
