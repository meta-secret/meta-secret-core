//! Genuine signed ServerApp transitions, never count-only lifecycle simulations.
use super::{fixture::*, observed_repo::Repo, support::*};
use anyhow::{Context, Result};
use meta_secret_core::node::db::{
    events::generic_log_event::{ObjIdExtractor, ToGenericEvent},
    repo::generic_db::FindOneQuery,
};
use meta_secret_core::node::{
    common::model::{
        liveboard_producer::*, meta_pass::PlainPassInfo, user::common::UserDataMember,
        vault::vault::VaultMember,
    },
    db::{
        actions::sign_up::join::{JoinAction, JoinActionUpdate},
        descriptors::{
            local_removal_envelope::LocalRemovalEnvelopeDescriptor,
            vault_descriptor::VaultLogDescriptor,
        },
        objects::persistent_device_log::PersistentDeviceLog,
        repo::persistent_credentials::PersistentCredentials,
    },
};
use std::sync::Arc;
const LONG: &str = "12345678901234567890123456789012345678901234567890123456789012345";
#[tokio::test]
async fn actual_nonempty_2to1_keeps_peer_and_current_created_ciphertext_and_becomes_ready()
-> Result<()> {
    let h = Harness::new(Arc::new(Repo::default()), 2).await?;
    drive(&h,async {
        h.web.get_state().await?;
        let created=h.service.cluster_distribution_checked(PlainPassInfo::new("current-65-byte".into(),LONG.into())).await?;
        h.web.sync_gateway.sync(h.actor.user()).await?;
        let vault=meta_secret_core::node::db::objects::persistent_vault::PersistentVault::from(h.p.clone()).get_vault(h.actor.vault_name.clone()).await?.0.value;
        let current=vault.secrets.iter().find(|p|Some(p.id_str())==created.secret_id).context("actual checked Create ID")?.clone();
        let before=vec![distribution(&h,&h.primary).await?,distribution(&h,&current).await?];
        let long=share(&h,&current).await?;
        assert_eq!(long.share_blocks.len(),2);
        assert_eq!(long.share_blocks.iter().map(|b|b.meta_data.size).collect::<Vec<_>>(),vec![64,1]);
        h.p.repo.reset_observation();
        let receipt=h.remove(false).await?;
        let sources=h.p.repo.sources();
        assert_eq!(sources.len(),2);
        assert_source(&receipt,first_request(&sources)?)?;
        let Some(ProducerDecisionStage::Attempted{source,..})=&receipt.decision_stage else {anyhow::bail!("actual U missing");};
        assert_eq!(source.event_id,sources[1].obj_id());
        assert!(before==vec![distribution(&h,&h.primary).await?,distribution(&h,&current).await?],"2to1 must not regenerate retained ciphertext or generation");
        show(&h,&h.primary,super::seed_fixture::VALUE).await?;
        show(&h,&current,LONG).await?;
        let state=public_project(&h).await?;
        eprintln!("WHOLE_COPY_2TO1 material_exact=true public_show=2 current_members=1 projected_ready={} receipt={:?}",state.secrets.iter().filter(|s|s.readiness=="ready").count(),receipt.phase);
        assert_eq!(state.storage_mode,"whole");
        assert!(ready(&state,&h.primary)&&ready(&state,&current),"RED: actual retained peer/current whole copies must remain ready");
        assert_eq!(receipt.phase,ProducerPhase::Queued);
        assert_eq!(receipt.material_stage,Some(ProducerMaterialStage::Ready));
        assert!(h.p.find_tail_event(LocalRemovalEnvelopeDescriptor(h.actor.user_id())).await?.and_then(|v|v.0.value.producer_context).is_none());
        Ok(())
    }).await
}
#[tokio::test]
async fn actual_signed_1to2_retained_n1_window_and_final_n2_agree_with_existing_show() -> Result<()>
{
    let h = Harness::new(Arc::new(Repo::default()), 1).await?;
    let candidate = Harness::reopen(
        Arc::new(Repo::default()),
        ReopenState {
            actor: h.target.clone(),
            target: h.actor.clone(),
            master: h.master.clone(),
            primary: h.primary.clone(),
            secondary: h.secondary.clone(),
            network: h.network.clone(),
        },
    )?;
    PersistentCredentials {
        p_obj: candidate.p.clone(),
        master_key: candidate.master.clone(),
    }
    .save_user_creds(candidate.actor.clone())
    .await?;
    PersistentDeviceLog::from(candidate.p.clone())
        .save_join_request(&candidate.actor.user())
        .await?;
    let join = PersistentDeviceLog::from(candidate.p.clone())
        .latest_join_request(&candidate.actor.user())
        .await?
        .context("actual generated Join")?;
    candidate
        .web
        .sync_gateway
        .sync(candidate.actor.user())
        .await?;
    h.web.sync_gateway.sync(h.actor.user()).await?;
    let vault =
        meta_secret_core::node::db::objects::persistent_vault::PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?
            .0
            .value;
    let log =
        h.p.find_tail_event(VaultLogDescriptor::from(h.actor.vault_name.clone()))
            .await?
            .context("canonical pending Join")?;
    assert!(log.0.value.requests.iter().any(|r|matches!(r,meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent::JoinCluster(actual) if *actual==join)));
    JoinAction {
        p_obj: h.p.clone(),
        member: VaultMember {
            member: UserDataMember::from(h.actor.user()),
            vault,
        },
    }
    .update(join, JoinActionUpdate::Accept)
    .await?;
    let original = distribution(&h, &h.primary).await?;
    assert_eq!(
        share(&h, &h.primary).await?.share_blocks[0]
            .config
            .number_of_shares,
        1
    );
    h.p.repo.inner.arm_join();
    let inspect = async {
        h.p.repo.inner.wait_join().await;
        let outcome = async {
            assert!(original == distribution(&h, &h.primary).await?);
            assert_eq!(
                share(&h, &h.primary).await?.share_blocks[0]
                    .config
                    .number_of_shares,
                1
            );
            show(&h, &h.primary, super::seed_fixture::VALUE).await?;
            let vault =
                meta_secret_core::node::db::objects::persistent_vault::PersistentVault::from(
                    h.p.clone(),
                )
                .get_vault(h.actor.vault_name.clone())
                .await?;
            assert_eq!(vault.0.value.members().len(), 2);
            readonly_available(&h, &h.primary).await
        }
        .await;
        h.p.repo.inner.release_join();
        outcome
    };
    let (_, window_ready) = tokio::try_join!(h.web.sync_gateway.sync(h.actor.user()), inspect)?;
    for client in [&h, &candidate] {
        let sources=client.p.get_object_events_from_beginning(meta_secret_core::node::db::descriptors::vault_descriptor::DeviceLogDescriptor::from(client.actor.user_id())).await?;
        assert!(!sources.is_empty());
        for source in sources {
            assert!(
                client.network.repo.find_one(source.obj_id()).await? == Some(source.to_generic()),
                "actual signed server must retain exact Join/approval source"
            );
        }
    }
    // Finish ordinary genuine redistribution even when the retained-window predicate is RED.
    h.web.sync_gateway.sync(h.actor.user()).await?;
    candidate
        .web
        .sync_gateway
        .sync(candidate.actor.user())
        .await?;
    for client in [&h, &candidate] {
        let actual = share(client, &client.primary).await?;
        assert_eq!(actual.share_blocks[0].config.number_of_shares, 2);
        assert_eq!(actual.share_blocks[0].config.threshold, 1);
        show(client, &client.primary, super::seed_fixture::VALUE).await?;
        assert!(ready(&public_project(client).await?, &client.primary));
    }
    assert!(
        original != distribution(&h, &h.primary).await?,
        "actual final join creates a fresh n2 generation"
    );
    eprintln!(
        "WHOLE_COPY_JOIN signed=true retained_n1=true final_n2_clients=2 public_show=true window_ready={window_ready}"
    );
    assert!(
        window_ready,
        "RED: actual accepted-member retained n1 full copy is unavailable to its no-reveal component"
    );
    Ok(())
}
