//! Direct instrumentation uses unchanged Core source functions and real stored records.
use super::{fixture::*, observed_repo::Repo};
use anyhow::{Context, Result};
use meta_secret_core::{
    node::{
        common::model::{
            liveboard::LiveboardState, meta_pass::MetaPasswordId, user::common::UserDataMember,
            vault::vault::VaultMember,
        },
        db::{
            descriptors::{shared_secret_descriptor::*, vault_descriptor::*},
            events::shared_secret_event::SsWorkflowObject,
        },
    },
    secret::shared_secret::UserShareDto,
};
use std::sync::atomic::Ordering;
pub async fn distribution(h: &Harness<Repo>, pass: &MetaPasswordId) -> Result<SsWorkflowObject> {
    h.p.find_tail_event(SsWorkflowDescriptor::Distribution(
        meta_secret_core::node::common::model::secret::SsDistributionId {
            pass_id: pass.clone(),
            receiver: h.actor.device_id().clone(),
        },
    ))
    .await?
    .context("exact own Distribution")
}
pub async fn share(h: &Harness<Repo>, pass: &MetaPasswordId) -> Result<UserShareDto> {
    let data = distribution(h, pass).await?.to_distribution_data()?;
    let clear = data
        .secret_message
        .cipher_text()
        .decrypt(&h.actor.device_creds.secret_box.transport.sk)?;
    Ok(UserShareDto::try_from(&clear.msg)?)
}
pub async fn public_project(h: &Harness<Repo>) -> Result<LiveboardState> {
    let actual = h.service.get_app_state().await?;
    let meta_secret_core::node::common::model::ApplicationState::Vault(
        meta_secret_core::node::common::model::VaultFullInfo::Member(member),
    ) = actual
    else {
        anyhow::bail!("actual current member projection");
    };
    member
        .liveboard_state
        .context("actual production projection")
}
pub async fn readonly_available(h: &Harness<Repo>, pass: &MetaPasswordId) -> Result<bool> {
    let vault =
        h.p.find_tail_event(VaultDescriptor::from(h.actor.vault_name.clone()))
            .await?
            .context("actual local Vault")?
            .0
            .value;
    let claims =
        h.p.find_tail_event(SsLogDescriptor::from(h.actor.vault_name.clone()))
            .await?
            .context("actual canonical SsLog")?
            .0
            .value;
    let member = VaultMember {
        member: UserDataMember::from(h.actor.user()),
        vault,
    };
    let before = h.p.repo.inner.snapshot().await;
    let writes = h.p.repo.inner.writes.load(Ordering::SeqCst);
    let network = h.network.counts();
    let material = super::liveboard_material::Material {
        p_obj: h.p.clone(),
        creds: &h.actor,
        member: &member,
        claims: &claims,
    };
    let ready = member
        .vault
        .ensure_local_secret_available(h.actor.device_id(), pass)
        .is_ok()
        && material.local(pass).await.is_ok_and(|v| v.is_some());
    assert!(
        before == h.p.repo.inner.snapshot().await,
        "availability must preserve exact local DB records"
    );
    assert_eq!(h.p.repo.inner.writes.load(Ordering::SeqCst), writes);
    assert_eq!(h.network.counts(), network);
    Ok(ready)
}
pub async fn show(h: &Harness<Repo>, pass: &MetaPasswordId, value: &str) -> Result<()> {
    let before = h.p.repo.inner.snapshot().await;
    let network = h.network.counts();
    assert!(
        h.web.show_local_secret(pass.clone()).await?.text == value,
        "actual Web Show mismatch"
    );
    assert!(
        h.mobile.show_local_secret(pass.clone()).await?.text == value,
        "actual common Show mismatch"
    );
    assert!(before == h.p.repo.inner.snapshot().await);
    assert_eq!(h.network.counts(), network);
    Ok(())
}
pub fn ready(state: &LiveboardState, pass: &MetaPasswordId) -> bool {
    state.secrets.iter().any(|s| {
        s.secret_id == pass.id_str() && s.primary_action == "showLocal" && s.readiness == "ready"
    })
}
