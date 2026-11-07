//! Actual private projection component; labelled shape fixtures, not public UI E2E.
use super::fixture::*;
use crate::node::{
    app::meta_app::liveboard_projection::project,
    db::events::vault::vault_log_event::VaultActionEvents,
    db::objects::persistent_vault::PersistentVault,
};
use anyhow::Result;
async fn check(stored: usize, current: usize) -> Result<()> {
    let h = Fixture::new(stored, current).await?;
    let before = h.p.repo.get_db().await;
    let vault_source = PersistentVault::from(h.p.clone())
        .get_vault(h.creds.vault_name.clone())
        .await?;
    let state = project(
        h.p.clone(),
        &h.creds,
        &h.member,
        &vault_source,
        &VaultActionEvents::default(),
        &h.claims,
    )
    .await?;
    assert!(
        before == h.p.repo.get_db().await,
        "pure projection must not write or finalize material"
    );
    let wire = serde_json::to_string(&state)?;
    for hidden in [
        VALUE,
        "shareBlocks",
        "secretMessage",
        "cipherText",
        "transportSk",
        "producerContext",
    ] {
        assert!(
            !wire.contains(hidden),
            "private material must not enter public DTO"
        );
    }
    let secret = state
        .secrets
        .iter()
        .find(|s| s.secret_id == h.pass.id_str())
        .unwrap();
    assert_eq!(
        secret.primary_action, "showLocal",
        "RED: pure projection must respect valid stored whole-copy generation"
    );
    assert_eq!(secret.readiness, "ready");
    Ok(())
}
#[tokio::test]
async fn valid_same_generation_projection_is_read_only_and_private() -> Result<()> {
    check(2, 2).await
}
#[tokio::test]
async fn retained_n1_at_current2_projection_is_read_only_and_ready() -> Result<()> {
    check(1, 2).await
}
#[tokio::test]
async fn retained_n2_at_current1_projection_is_read_only_and_ready() -> Result<()> {
    check(2, 1).await
}
