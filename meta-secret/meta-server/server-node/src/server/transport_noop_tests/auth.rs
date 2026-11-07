//! Valid-variant gate precedes each negative: unknown variant is NOT crypto evidence.
use super::fixture::*;
use anyhow::Result;
use meta_secret_core::node::{
    common::model::IdString,
    db::{
        descriptors::vault_descriptor::DeviceLogDescriptor,
        events::object_id::{ArtifactId, Next},
    },
};
use serde_json::json;
enum Bad {
    Key,
    Actor,
    ForeignStream,
    Vault,
    Purpose,
    Version,
    Nonce,
    Future,
    Sequence,
    Digest,
    Extra,
    Deep,
    Outsider,
}
async fn reject(case: Bad) -> Result<()> {
    let h = H::new().await?;
    let mut value = h.authorization(&h.a).await?;
    decode(value.clone())?; // meaningful schema gate; later verifier assertions unreached until additive type exists.
    match case {
        Bad::Key => {
            value = sign(&h.b, value)?;
        }
        Bad::Actor => {
            value["actorId"] = serde_json::to_value(h.b.device_id())?;
            value = sign(&h.a, value)?;
        }
        Bad::ForeignStream => {
            let id = ArtifactId::from(DeviceLogDescriptor::from(h.b.user_id()));
            value["eventId"] = serde_json::to_value(&id)?;
            value["stream"] = json!(id.fqdn.id_str());
            value = sign(&h.a, value)?;
        }
        Bad::Vault => {
            value["vaultName"] = json!("a-different-vault");
            value = sign(&h.a, value)?;
        }
        Bad::Purpose => {
            value["purpose"] = json!("meta-secret.event.v1");
            value = sign(&h.a, value)?;
        }
        Bad::Version => {
            value["version"] = json!(2);
            value = sign(&h.a, value)?;
        }
        Bad::Nonce => {
            value["nonce"] = json!(2);
            value = sign(&h.a, value)?;
        }
        Bad::Future => {
            let id: ArtifactId = serde_json::from_value(value["eventId"].clone())?;
            let future = id.next();
            value["eventId"] = serde_json::to_value(&future)?;
            value["nonce"] = json!(future.id.curr);
            value = sign(&h.a, value)?;
        }
        Bad::Sequence => {
            value["eventId"]["id"]["curr"] = json!(2);
            value["nonce"] = json!(2);
            value = sign(&h.a, value)?;
        }
        Bad::Digest => {
            value["clientEntryCommitment"] = json!("a".repeat(4097));
            value = sign(&h.a, value)?;
        }
        Bad::Extra => {
            value["requestId"] = json!("PRIVATE-FORBIDDEN-PAYLOAD");
            value = sign(&h.a, value)?;
        }
        Bad::Deep => {
            let mut nested = json!("PRIVATE-FORBIDDEN-PAYLOAD");
            for _ in 0..140 {
                nested = json!([nested]);
            }
            value["evidence"] = nested;
            value = sign(&h.a, value)?;
        }
        Bad::Outsider => {
            value = h.authorization(&h.outsider).await?;
        }
    }
    let before = h.p.repo.inner.get_db().await;
    let result = match decode(value) {
        Ok(request) => h.server.handle_client_request(request).await.map(|_| ()),
        Err(error) => Err(error),
    };
    assert!(
        result.is_err(),
        "dedicated parser/current-key/own-next-slot gate must reject invalid input"
    );
    assert!(
        before == h.p.repo.inner.get_db().await,
        "denied technical write must have no storage/domain effects"
    );
    Ok(())
}
macro_rules! bad {
    ($name:ident,$case:ident) => {
        #[tokio::test]
        async fn $name() -> Result<()> {
            reject(Bad::$case).await
        }
    };
}
bad!(wrong_registered_key_cannot_authorize_actor_slot, Key);
bad!(
    caller_actor_id_cannot_replace_registered_signature_owner,
    Actor
);
bad!(
    foreign_device_stream_and_artifact_cannot_be_filled,
    ForeignStream
);
bad!(cross_vault_name_has_no_current_actor_authority, Vault);
bad!(ordinary_event_purpose_cannot_be_reused_for_noop, Purpose);
bad!(unsupported_signed_version_is_rejected, Version);
bad!(nonce_must_equal_complete_artifact_sequence, Nonce);
bad!(
    future_valid_own_sequence_cannot_create_transport_hole,
    Future
);
bad!(malformed_curr_prev_sequence_is_not_progress, Sequence);
bad!(oversized_client_digest_is_rejected, Digest);
bad!(
    extra_request_payload_is_rejected_before_domain_handling,
    Extra
);
bad!(deeply_nested_hidden_evidence_is_rejected, Deep);
bad!(canonical_outsider_has_no_own_slot_permission, Outsider);
#[tokio::test]
async fn removed_actor_cannot_replay_even_previously_committed_exact_noop() -> Result<()> {
    let h = H::new().await?;
    let auth = h.authorization(&h.b).await?;
    h.submit(auth.clone()).await?;
    let request = h.removal();
    h.business(&h.a,meta_secret_core::node::db::events::vault::vault_log_event::VaultActionEvent::Request(meta_secret_core::node::db::events::vault::vault_log_event::VaultActionRequestEvent::RemoveMember(request.clone()))).await?;
    let decision=meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberEvent{request,sender:h.c.user().into(),target:h.b.user().into(),new_status:meta_secret_core::node::common::model::user::common::UserMembership::Outsider(meta_secret_core::node::common::model::user::common::UserDataOutsider{user_data:h.b.user(),status:meta_secret_core::node::common::model::user::common::UserDataOutsiderStatus::Declined}),decision:meta_secret_core::node::db::events::vault::vault_log_event::RemoveMemberDecision::Accepted,share_transfers:vec![]};
    h.business(&h.c,meta_secret_core::node::db::events::vault::vault_log_event::VaultActionEvent::Update(meta_secret_core::node::db::events::vault::vault_log_event::VaultActionUpdateEvent::RemoveMember(decision))).await?;
    let vault =
        h.p.find_tail_event(
            meta_secret_core::node::db::descriptors::vault_descriptor::VaultDescriptor::from(
                h.a.vault_name.clone(),
            ),
        )
        .await?
        .unwrap();
    assert!(!vault.0.value.is_member(h.b.device_id()));
    let before = h.p.repo.inner.get_db().await;
    assert!(h.submit(auth).await.is_err());
    assert!(before == h.p.repo.inner.get_db().await);
    Ok(())
}
