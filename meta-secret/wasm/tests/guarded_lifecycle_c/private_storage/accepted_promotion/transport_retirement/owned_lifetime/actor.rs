//! Canonical INITIAL fixture is reused, but B imports only real addressed responses.
use super::*;
use meta_secret_core::{
    crypto::key_pair::{KeyPair, TransportDsaKeyPair},
    node::db::repo::persistent_credentials::PersistentCredentials,
};

pub(super) async fn case() -> Result<(Case, Arc<local_repo::Repo>)> {
    // INITIAL three-member/ciphertext fixture, not a signed initial Join lifecycle.
    // Its subsequent A Remove request and every read/decision below are real.
    let seed = super::super::super::super::super::fixture::Harness::new().await?;
    let state = State {
        a: seed.a.clone(),
        b: seed.b.clone(),
        c: seed.c.clone(),
        passes: seed.passes.clone(),
        uuid: seed.request_id.clone(),
        master: TransportDsaKeyPair::generate().sk(),
        server: seed.network.server.clone(),
        server_repo: seed.server_repo.clone(),
        server_master: seed.server_master.clone(),
    };
    let local = Arc::new(local_repo::Repo::new());
    ensure!(
        local.inner.get_db().await.is_empty(),
        "B local repository must start empty"
    );
    let observations = Arc::new(Observations::default());
    let observed = Arc::new(super::super::super::repo::Observed::new(
        local.clone(),
        observations.clone(),
    ));
    let existing = Arc::new(ExistingRepo::new(
        observed,
        state.b.user_id(),
        state.uuid.clone(),
    ));
    let h = H::open(existing, state)?;
    PersistentCredentials {
        p_obj: h.p.clone(),
        master_key: h.state.master.clone(),
    }
    .save_user_creds(h.state.b.clone())
    .await?;

    let tail = PersistentVault::from(h.p.clone())
        .vault_tail(h.state.b.user())
        .await?;
    let vault = h
        .wire
        .send(SyncRequest::Read(Box::new(ReadSyncRequest::Vault(
            VaultRequest {
                sender: h.state.b.user(),
                tail,
            },
        ))))
        .await?
        .to_data()?;
    ensure!(
        !vault.0.is_empty(),
        "actual initial addressed B Vault response must be nonempty"
    );
    for event in vault.0 {
        h.p.repo.save(event).await?;
    }

    // Copy only B's actual admitted source prefix, if present. Never a seed DB/raw map.
    let server = PersistentObject::new(h.state.server_repo.clone());
    let bodies: Vec<DeviceLogObject> = server
        .get_object_events_from_beginning(DeviceLogDescriptor::from(h.state.b.user_id()))
        .await?;
    for body in bodies {
        h.p.repo.save(body).await?;
    }

    let ss_log =
        h.p.find_free_id_by_obj_desc(SsLogDescriptor::from(h.state.b.vault_name.clone()))
            .await?;
    let ss = h
        .wire
        .send(SyncRequest::Read(Box::new(ReadSyncRequest::SsRequest(
            SsRequest {
                sender: h.state.b.user(),
                ss_log,
            },
        ))))
        .await?
        .to_data()?;
    ensure!(
        !ss.0.is_empty(),
        "actual initial addressed B SS response must be nonempty"
    );
    let returned = ss.0.clone();
    for event in ss.0 {
        h.p.repo.save(event).await?;
    }
    initial_actor(&h, &returned).await?;
    h.p.repo.trace.lock().unwrap().clear();
    h.p.repo.before_body.lock().unwrap().clear();
    Ok((Case::from_h(h, observations), local))
}

async fn initial_actor(
    h: &H<super::super::super::repo::Observed<local_repo::Repo>>,
    returned: &[GenericKvLogEvent],
) -> Result<()> {
    let snapshot = h.p.repo.snapshot().await?;
    let packets = snapshot
        .values()
        .filter_map(|event| match event {
            GenericKvLogEvent::SsWorkflow(packet @ SsWorkflowObject::Distribution(_)) => {
                Some(packet)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        packets.len() == 2,
        "actual B actor must import exactly two current Pass inputs"
    );
    for packet in packets {
        let data = packet.clone().to_distribution_data()?;
        let channel = &data.secret_message.cipher_text().channel;
        ensure!(
            data.vault_name == h.state.b.vault_name
                && h.state.passes.contains(&data.claim_id.pass_id)
                && channel.receiver() == &h.state.b.user().device.keys.transport_pk
                && channel.sender() == &h.state.a.user().device.keys.transport_pk
                && packet.obj_id() == packet_id(&data.claim_id.pass_id, &h.state.b)
                && returned.contains(&packet.clone().to_generic()),
            "initial local Distribution is not exact actually addressed B input"
        );
        let plain = data
            .secret_message
            .cipher_text()
            .decrypt(&h.state.b.device_creds.secret_box.transport.sk)?;
        let share = UserShareDto::try_from(&plain.msg)?;
        ensure!(
            !share.share_blocks.is_empty()
                && share
                    .share_blocks
                    .iter()
                    .all(|b| b.config.threshold == 2 && b.config.number_of_shares == 3),
            "initial B input is not actual K2-of3"
        );
        ensure!(
            recover_from_shares(vec![share]).is_err(),
            "actual initial B singleton must not recover"
        );
    }
    let actual = PersistentVault::from(h.p.clone())
        .get_vault(h.state.b.vault_name.clone())
        .await?;
    let log = PersistentVault::from(h.p.clone())
        .get_vault_log_artifact(h.state.b.vault_name.clone())
        .await?;
    ensure!(
        actual.0.value.members().len() == 3
            && actual.0.value.secrets.len() == 2
            && log
                .0
                .value
                .liveboard_removal_pending
                .iter()
                .any(|p| p.request_id == h.state.uuid),
        "actual initial B Vault/current two-Pass/pending source import differs"
    );
    eprintln!(
        "OWNED_LIFETIME ACTOR empty_B_start=true actual_nonempty_Vault_SS_import=true initial_all_Distribution_receivers_B=true current_two_Pass_K2=true singleton_Err=true INITIAL_membership_is_fixture=true"
    );
    Ok(())
}
