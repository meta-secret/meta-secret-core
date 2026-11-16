//! Exact snapshot Claim wrapper tests; no new remote response authority.
#![cfg(not(target_arch = "wasm32"))]
#[path = "../../core/tests/liveboard_ports/fault_repo.rs"]
#[allow(dead_code)]
mod fault_repo;
#[path = "../../core/tests/liveboard_ports/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;
#[path = "../../core/tests/guarded_removal_outbox/fixtures.rs"]
#[allow(dead_code)]
mod transport;

use anyhow::{Context, Result};
use fixtures::Harness;
use meta_secret_core::{
    crypto::utils::Id48bit,
    node::{
        common::model::{
            ApplicationState, UserMemberFullInfo, VaultFullInfo, WasmApplicationState,
            secret::{ClaimId, SecretDistributionType, SsClaim},
            user::common::UserDataMember,
            vault::vault::VaultName,
        },
        db::objects::persistent_shared_secret::PersistentSharedSecret,
    },
};

async fn state_with_two_same_pass_claims() -> Result<(Harness, ApplicationState, SsClaim, SsClaim)>
{
    let mut h = Harness::new(3).await?;
    let pass = h.seed_secret("same-pass-exact-claim").await?;
    let vm = h
        .vault
        .clone()
        .to_vault_member(UserDataMember::from(h.approver.user()))?;
    let first = vm.create_recovery_claim(pass.clone());
    let second = vm.create_recovery_claim(pass);
    assert_ne!(first.id, second.id);
    let ss = PersistentSharedSecret::from(h.p.clone());
    ss.save_ss_log_event(first.clone()).await?;
    ss.save_ss_log_event(second.clone()).await?;
    let state = h.service().get_app_state().await?;
    Ok((h, state, first, second))
}
fn wrapper(
    member: UserMemberFullInfo,
) -> meta_secret_core::node::common::model::WasmUserMemberFullInfo {
    WasmApplicationState::from(ApplicationState::Vault(VaultFullInfo::Member(Box::new(
        member,
    ))))
    .as_vault()
    .as_member()
}

#[tokio::test]
async fn exact_second_same_pass_claim_matches_native_string_and_outlives_snapshot() -> Result<()> {
    let (h, state, first, second) = state_with_two_same_pass_claims().await?;
    let before = h.p.repo.get_db().await;
    let native_json = serde_json::to_value(&state)?;
    let key = second.id.wasm_id_str();
    assert!(
        native_json["vault"]["member"]["ssClaims"]["claims"]
            .get(&key)
            .is_some(),
        "native state must contain the exact real second key"
    );
    let member = WasmApplicationState::from(state).as_vault().as_member();
    let selected = member
        .recovery_claim_by_id(&key)
        .context("RED: exact second Claim wrapper missing")?;
    let selected_again = member
        .recovery_claim_by_id(&key)
        .context("independent owned clone")?;
    let native_exact = ClaimId::from(Id48bit::from(key.clone()));
    assert_eq!(selected, native_exact);
    assert_eq!(selected_again, second.id);
    assert_ne!(
        selected, first.id,
        "same Pass must never select another historical Claim"
    );
    drop(member);
    assert_eq!(
        selected.wasm_id_str(),
        key,
        "owned result must remain usable after its snapshot drops"
    );
    drop(selected_again);
    assert_eq!(selected, second.id);
    assert!(
        h.p.repo.get_db().await == before,
        "lookup must not sync, write or retire a claim"
    );
    Ok(())
}

#[tokio::test]
async fn exact_claim_getter_rejects_unknown_empty_unbounded_or_noncanonical_input_without_writes()
-> Result<()> {
    let (h, state, first, _) = state_with_two_same_pass_claims().await?;
    let before = h.p.repo.get_db().await;
    let member = WasmApplicationState::from(state).as_vault().as_member();
    for input in [
        String::new(),
        "unknown-claim".into(),
        "x".repeat(513),
        format!(" {}", first.id.wasm_id_str()),
        format!("{} ", first.id.wasm_id_str()),
    ] {
        assert!(member.recovery_claim_by_id(&input).is_none());
    }
    assert!(h.p.repo.get_db().await == before);
    Ok(())
}

#[tokio::test]
async fn exact_claim_key_dist_identity_vault_and_recover_kind_must_all_agree() -> Result<()> {
    let (h, state, first, second) = state_with_two_same_pass_claims().await?;
    let ApplicationState::Vault(VaultFullInfo::Member(member)) = state else {
        anyhow::bail!("real member missing")
    };
    let id = first.id.wasm_id_str();
    for case in 0..4 {
        let mut corrupt = (*member).clone();
        let mut claim = corrupt
            .ss_claims
            .claims
            .remove(&first.id)
            .context("real claim")?;
        match case {
            0 => claim.id = second.id.clone(),
            1 => claim.dist_claim_id.id = second.id.clone(),
            2 => claim.vault_name = VaultName::from("different-vault"),
            _ => claim.distribution_type = SecretDistributionType::Split,
        }
        corrupt.ss_claims.claims.insert(first.id.clone(), claim);
        assert!(
            wrapper(corrupt).recovery_claim_by_id(&id).is_none(),
            "contradictory native snapshot must not mint an exact wrapper"
        );
    }
    assert!(
        wrapper((*member).clone())
            .recovery_claim_by_id(&id)
            .is_some(),
        "RED: valid control must resolve an exact real key"
    );
    assert!(!h.p.repo.get_db().await.is_empty());
    Ok(())
}
