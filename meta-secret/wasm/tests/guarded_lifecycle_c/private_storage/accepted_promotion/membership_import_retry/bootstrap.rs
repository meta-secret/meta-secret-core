use super::*;

pub(super) struct World {
    pub a: client::Client<InMemKvLogEventRepo>,
    pub b: client::Client<file_repo::Repo>,
    pub c: client::Client<InMemKvLogEventRepo>,
    pub remote: Remote,
    pub server_repo: Arc<InMemKvLogEventRepo>,
    pub pass: MetaPasswordId,
    pub original: SsWorkflowObject,
    pub original_claim: SsClaim,
}
async fn approve<R: KvLogEventRepo>(
    a: &client::Client<InMemKvLogEventRepo>,
    candidate: &client::Client<R>,
    server: &Arc<InMemKvLogEventRepo>,
) -> Result<String> {
    PersistentDeviceLog::from(candidate.p.clone())
        .save_join_request(&candidate.creds.user())
        .await?;
    let request = PersistentDeviceLog::from(candidate.p.clone())
        .latest_join_request(&candidate.creds.user())
        .await?
        .context("actual generated Join UUID")?;
    let body = candidate.send_latest(server).await?;
    ensure!(
        matches!(&body.0.value,VaultActionEvent::Request(VaultActionRequestEvent::JoinCluster(r)) if r==&request),
        "actual Join source differs"
    );
    a.pull_vault().await?;
    let prefix = PersistentDeviceLog::from(a.p.clone())
        .find_tail_id(&a.creds.user_id())
        .await?
        .context("actual A prefix before approval")?;
    MetaOrchestrator {
        p_obj: a.p.clone(),
        user_creds: a.creds.clone(),
    }
    .update_membership(request.clone(), JoinActionUpdate::Accept)
    .await?;
    let update = a.send_latest(server).await?;
    ensure!(
        update.obj_id().id.curr == prefix.id.curr + 1
            && matches!(&update.0.value,VaultActionEvent::Update(VaultActionUpdateEvent::UpdateMembership(u))
            if u.request==request && matches!(&u.update,UserMembership::Member(m) if m.user_data==candidate.creds.user())),
        "actual A approval must preserve its real prefix and exact candidate request"
    );
    Ok(request.request_id)
}
pub(super) async fn setup(url: String) -> Result<World> {
    let seed = FixtureRegistry::empty().state;
    let server_repo = Arc::new(InMemKvLogEventRepo::default());
    let remote = Arc::new(ServerApp::new(
        server_repo.clone(),
        seed.device_creds.server_master_key.clone(),
    )?);
    remote.init().await?;
    let a = client::Client::new(
        Arc::new(InMemKvLogEventRepo::default()),
        seed.user_creds.client,
        remote.clone(),
    )
    .await?;
    EmbeddedMigrationsTool {
        db_url: url.clone(),
    }
    .migrate();
    let b = client::Client::new(
        Arc::new(file_repo::Repo::open(url)),
        seed.user_creds.client_b,
        remote.clone(),
    )
    .await?;
    let c = client::Client::new(
        Arc::new(InMemKvLogEventRepo::default()),
        seed.user_creds.vd,
        remote.clone(),
    )
    .await?;
    PersistentDeviceLog::from(a.p.clone())
        .save_create_vault_request(&a.creds.user())
        .await?;
    let created = a.send_latest(&server_repo).await?;
    ensure!(
        matches!(created.0.value, VaultActionEvent::Init(_)),
        "actual Create source required"
    );
    approve(&a, &b, &server_repo).await?;
    b.pull_vault().await?;
    let vault = PersistentVault::from(b.p.clone())
        .get_vault(b.creds.vault_name.clone())
        .await?;
    ensure!(
        vault.0.value.members().len() == 2
            && vault.0.value.is_member(a.creds.device_id())
            && vault.0.value.is_member(b.creds.device_id())
            && !vault.0.value.is_member(c.creds.device_id()),
        "actual Create and B Join must establish exactly A/B"
    );
    let plain = PlainPassInfo::new("join-import-owned".into(), VALUE.into());
    let pass = plain.pass_id.clone();
    let member = VaultMember {
        member: b.creds.user().into(),
        vault: vault.0.value,
    };
    MetaDistributor {
        p_obj: b.p.clone(),
        user_creds: Arc::new(b.creds.clone()),
        vault_member: member.clone(),
    }
    .distribute(member, SecurePassInfo::from(plain))
    .await?;
    b.sync().await?;
    let just_uploaded = proof::packet(&b, &pass, b.creds.device_id())
        .await?
        .context("actual own material after first B raw upload")?;
    let just_observed_claim = proof::active_claim(&b, &just_uploaded).await?;
    let a_status = just_observed_claim.status.get(a.creds.device_id());
    let outgoing_a_absent = proof::packet(&b, &pass, a.creds.device_id())
        .await?
        .is_none();
    eprintln!(
        "JoinImport metadata_AFTER_FIRST_actual_B_raw_sync local_A_Pending={} local_A_Sent={} local_A_Delivered={} B_local_outgoing_A_absent={outgoing_a_absent}",
        matches!(a_status, Some(SsDistributionStatus::Pending)),
        matches!(a_status, Some(SsDistributionStatus::Sent)),
        matches!(a_status, Some(SsDistributionStatus::Delivered))
    );
    // Finish real original peer delivery before Join; no stale outbound packet seam.
    b.sync().await?;
    a.pull_vault().await?;
    a.pull_ss().await?;
    let original = proof::packet(&b, &pass, b.creds.device_id())
        .await?
        .context("actual B-owned original copy")?;
    let original_claim = proof::active_claim(&b, &original).await?;
    ensure!(
        original_claim.sender == *b.creds.device_id()
            && original_claim.receivers == vec![a.creds.device_id().clone()],
        "B must be actual canonical sender, not a receiver"
    );
    proof::config(&proof::share(&original, &b.creds)?, 1, 2)?;
    ensure!(
        proof::remote_packet(&server_repo, &pass, b.creds.device_id())
            .await?
            .is_none(),
        "sender's own original share must not be a remote pending packet"
    );
    let c_join = approve(&a, &c, &server_repo).await?;
    let canonical = PersistentVault::from(Arc::new(PersistentObject::new(server_repo.clone())))
        .get_vault(b.creds.vault_name.clone())
        .await?;
    ensure!(
        canonical.0.value.members().len() == 3 && canonical.0.value.is_member(c.creds.device_id()),
        "real A approval did not admit C"
    );
    let log = PersistentVault::from(Arc::new(PersistentObject::new(server_repo.clone())))
        .get_vault_log_artifact(b.creds.vault_name.clone())
        .await?;
    ensure!(
        log.0
            .value
            .join_results
            .iter()
            .any(|r| r.request.request_id == c_join && r.outcome == JoinResultOutcome::Accepted),
        "actual C Join result missing"
    );
    let local = PersistentVault::from(b.p.clone())
        .get_vault(b.creds.vault_name.clone())
        .await?;
    ensure!(
        local.0.value.members().len() == 2
            && proof::packet(&b, &pass, c.creds.device_id())
                .await?
                .is_none()
            && proof::remote_packet(&server_repo, &pass, c.creds.device_id())
                .await?
                .is_none(),
        "A must not reshare B-owned Secret; B must still observe two members"
    );
    b.p.repo.writes.lock().unwrap().clear();
    eprintln!(
        "JoinImport genuine Create/A_B_Join/B_owned_Secret/C_Join_A_approval=true localB_members2_server3=true actual_original_delivery_drained=true"
    );
    Ok(World {
        a,
        b,
        c,
        remote,
        server_repo,
        pass,
        original,
        original_claim,
    })
}
