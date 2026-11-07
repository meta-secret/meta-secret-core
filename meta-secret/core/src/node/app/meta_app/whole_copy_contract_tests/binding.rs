//! Invalid-data/context fixtures. These edits do not represent signed domain transitions.
use super::fixture::*;
use crate::node::{
    common::model::{secret::*, user::common::*, vault::vault::VaultName},
    db::events::shared_secret_event::SsWorkflowObject,
};
use anyhow::Result;
macro_rules! claim_edit {
    ($name:ident,$edit:expr) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            let mut h = Fixture::new(2, 2).await?;
            let c = h.claims.claims.values_mut().next().unwrap();
            ($edit)(c);
            h.denied().await
        }
    };
}
#[tokio::test]
async fn wrong_canonical_split_id() -> Result<()> {
    let mut h = Fixture::new(2, 2).await?;
    let other = h.member.create_split_claim(h.pass.clone()).id;
    h.claims.claims.values_mut().next().unwrap().id = other;
    h.denied().await
}
claim_edit!(wrong_canonical_vault, |c: &mut SsClaim| c.vault_name =
    VaultName::from("other-vault"));
claim_edit!(wrong_canonical_pass, |c: &mut SsClaim| {
    c.dist_claim_id.pass_id =
        crate::node::common::model::meta_pass::MetaPasswordId::build_from_str("other-pass")
});
claim_edit!(
    recover_claim_cannot_authorize_local_distribution,
    |c: &mut SsClaim| c.distribution_type = SecretDistributionType::Recover
);
#[tokio::test]
async fn no_canonical_claim_remains_unready_for_projection() -> Result<()> {
    let mut h = Fixture::new(1, 1).await?;
    h.claims.claims.clear();
    h.denied().await
}
#[tokio::test]
async fn canonical_map_key_must_match_claim_id() -> Result<()> {
    let mut h = Fixture::new(2, 2).await?;
    let (_, c) = h.claims.claims.drain().next().unwrap();
    let other = h.member.create_split_claim(h.pass.clone()).id;
    assert_ne!(other, c.id);
    h.claims.claims.insert(other, c);
    h.denied().await
}
#[tokio::test]
async fn wrong_distribution_variant_at_exact_typed_key_is_unavailable() -> Result<()> {
    let h = Fixture::new(2, 2).await?;
    let SsWorkflowObject::Distribution(e) = h.event().await? else {
        unreachable!()
    };
    h.replace(SsWorkflowObject::Recovery(e)).await?;
    h.denied().await
}
#[tokio::test]
async fn wrong_descriptor_inside_exact_saved_artifact_is_unavailable() -> Result<()> {
    let h = Fixture::new(2, 2).await?;
    let SsWorkflowObject::Distribution(mut e) = h.event().await? else {
        unreachable!()
    };
    let foreign =
        crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Distribution(
            SsDistributionId {
                pass_id: h.pass.clone(),
                receiver: h.peer.device_id().clone(),
            },
        );
    e.key.obj_desc =
        crate::node::db::descriptors::object_descriptor::ToObjectDescriptor::to_obj_desc(foreign);
    h.replace(SsWorkflowObject::Distribution(e)).await?;
    h.denied().await
}
#[tokio::test]
async fn ciphertext_intended_for_peer_is_not_own_material_even_if_sender_can_decrypt() -> Result<()>
{
    let h = Fixture::new(2, 2).await?;
    let peer=h.p.find_tail_event(crate::node::db::descriptors::shared_secret_descriptor::SsWorkflowDescriptor::Distribution(SsDistributionId{pass_id:h.pass.clone(),receiver:h.peer.device_id().clone()})).await?.unwrap().to_distribution_data()?;
    assert!(
        peer.secret_message
            .cipher_text()
            .decrypt(&h.creds.device_creds.secret_box.transport.sk)
            .is_ok()
    );
    let SsWorkflowObject::Distribution(mut own) = h.event().await? else {
        unreachable!()
    };
    own.value.secret_message = peer.secret_message;
    h.replace(SsWorkflowObject::Distribution(own)).await?;
    h.denied().await
}
#[tokio::test]
async fn wrong_data_pass_and_vault_are_rejected() -> Result<()> {
    for wrong_vault in [false, true] {
        let h = Fixture::new(2, 2).await?;
        let SsWorkflowObject::Distribution(mut e) = h.event().await? else {
            unreachable!()
        };
        if wrong_vault {
            e.value.vault_name = VaultName::from("other-vault");
        } else {
            e.value.claim_id.pass_id =
                crate::node::common::model::meta_pass::MetaPasswordId::build_from_str("other-pass");
        }
        h.replace(SsWorkflowObject::Distribution(e)).await?;
        h.denied().await?;
    }
    Ok(())
}
#[tokio::test]
async fn tombstoned_exact_pass_cannot_use_retained_material() -> Result<()> {
    let mut h = Fixture::new(2, 2).await?;
    h.member.vault = h.member.vault.clone().delete_secret(h.pass.clone());
    h.denied().await
}
#[tokio::test]
async fn removed_actor_cannot_use_retained_whole_copy() -> Result<()> {
    let mut h = Fixture::new(2, 2).await?;
    h.member.vault = h
        .member
        .vault
        .clone()
        .update_membership(UserMembership::Outsider(UserDataOutsider {
            user_data: h.creds.user(),
            status: UserDataOutsiderStatus::Declined,
        }));
    h.denied().await
}
#[tokio::test]
async fn valid_whole_copy_origin_is_not_a_current_role_or_mutable_split_owner_gate() -> Result<()> {
    let mut h = Fixture::new(2, 2).await?;
    let old = h.event().await?;
    // Unit shape mirrors the documented administrative owner reassignment, not a lifecycle claim.
    let SsWorkflowObject::Distribution(mut own) = old else {
        unreachable!()
    };
    let clear = own
        .value
        .secret_message
        .cipher_text()
        .decrypt(&h.creds.device_creds.secret_box.transport.sk)?;
    own.value.secret_message =
        crate::node::common::model::crypto::aead::EncryptedMessage::CipherShare {
            share: h
                .peer
                .device_creds
                .key_manager()?
                .transport
                .encrypt_string(
                    crate::secret::shared_secret::PlainText::from(&clear.msg),
                    &h.creds.device().keys.transport_pk,
                )?,
        };
    h.replace(SsWorkflowObject::Distribution(own)).await?;
    h.claims.claims.values_mut().next().unwrap().sender = h.creds.device_id().clone();
    h.member.vault = h
        .member
        .vault
        .clone()
        .update_membership(UserMembership::Outsider(UserDataOutsider {
            user_data: h.peer.user(),
            status: UserDataOutsiderStatus::Declined,
        }));
    assert!(
        h.material().local(&h.pass).await.is_ok_and(|v| v.is_some()),
        "RED: valid full-copy origin supplies no current authority"
    );
    Ok(())
}
