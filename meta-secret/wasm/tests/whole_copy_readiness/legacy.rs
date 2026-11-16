//! Explicit legacy/corrupt-record fixtures, not membership lifecycle claims.
use super::{fixture::*, observed_repo::Repo, support::*};
use anyhow::Result;
use meta_secret_core::node::{
    common::model::{liveboard_producer::ProducerCanonicalState, meta_pass::PlainPassInfo},
    db::{
        descriptors::shared_secret_descriptor::SsLogDescriptor,
        events::generic_log_event::ObjIdExtractor, repo::generic_db::*,
    },
};
use std::sync::Arc;
#[tokio::test]
async fn existing_web_and_common_show_do_not_require_new_canonical_claim_gate() -> Result<()> {
    let h = Harness::new(Arc::new(Repo::default()), 1).await?;
    for event in
        h.p.get_object_events_from_beginning(SsLogDescriptor::from(h.actor.vault_name.clone()))
            .await?
    {
        h.p.repo.delete(event.obj_id()).await;
    }
    show(&h, &h.primary, super::seed_fixture::VALUE).await?;
    // Projection's stronger canonical claim requirement remains separate from OLD Show.
    let vault =
        meta_secret_core::node::db::objects::persistent_vault::PersistentVault::from(h.p.clone())
            .get_vault(h.actor.vault_name.clone())
            .await?
            .0
            .value;
    let member = meta_secret_core::node::common::model::vault::vault::VaultMember {
        member: h.actor.user().into(),
        vault,
    };
    let claims = Default::default();
    let material = super::liveboard_material::Material {
        p_obj: h.p.clone(),
        creds: &h.actor,
        member: &member,
        claims: &claims,
    };
    assert!(material.local(&h.primary).await.is_err());
    let recovery = member.create_recovery_claim(h.primary.clone());
    assert!(
        material.recovered(&recovery, None).await.is_err(),
        "no approval cannot become recovered material"
    );
    Ok(())
}
#[tokio::test]
async fn actual_deleted_same_name_new_id_never_falls_back_to_old_ciphertext() -> Result<()> {
    let h = Harness::new(Arc::new(Repo::default()), 1).await?;
    drive(&h, async {
        h.web.get_state().await?;
        let removed = h.delete(false, h.primary.clone()).await?;
        assert_eq!(removed.canonical_state, ProducerCanonicalState::Applied);
        let created = h
            .service
            .cluster_distribution_checked(PlainPassInfo::new(
                h.primary.name.clone(),
                super::seed_fixture::VALUE.into(),
            ))
            .await?;
        h.web.sync_gateway.sync(h.actor.user()).await?;
        let state = public_project(&h).await?;
        let id = created.secret_id.as_ref().expect("actual new exact ID");
        assert_ne!(*id, h.primary.id_str());
        assert!(
            state
                .secrets
                .iter()
                .all(|s| s.secret_id != h.primary.id_str())
        );
        assert!(
            state
                .secrets
                .iter()
                .any(|s| s.secret_id == *id && s.primary_action == "showLocal")
        );
        assert!(h.web.show_local_secret(h.primary.clone()).await.is_err());
        assert!(h.mobile.show_local_secret(h.primary.clone()).await.is_err());
        Ok(())
    })
    .await
}
