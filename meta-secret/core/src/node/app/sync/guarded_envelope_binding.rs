//! Indexed Guarded ownership cannot fall back to a Legacy signing envelope.
use super::{sync_gateway::SyncGateway, sync_protocol::SyncProtocol};
use crate::node::{
    common::model::user::common::UserData,
    db::{
        events::{
            local_removal_envelope::{LocalRemovalEnvelopeEntry, RemovalEnvelopeMode},
            object_id::ArtifactId,
        },
        objects::local_guarded_preparation::{PersistentGuardedPreparation, UNAVAILABLE},
        repo::generic_db::KvLogEventRepo,
    },
};
use anyhow::{Result, ensure};

impl<R: KvLogEventRepo, S: SyncProtocol> SyncGateway<R, S> {
    pub(super) async fn validate_guarded_envelope_binding(
        &self,
        user: &UserData,
        id: &ArtifactId,
        entry: Option<&LocalRemovalEnvelopeEntry>,
    ) -> Result<()> {
        let store = PersistentGuardedPreparation {
            p_obj: self.p_obj.clone(),
        };
        let mut request_id = None;
        for object in store.load_all(&user.user_id()).await? {
            let value = &object.0.value;
            for origin in value.origin.iter().chain(
                value
                    .previous_attempts
                    .iter()
                    .map(|attempt| &attempt.origin),
            ) {
                if &origin.event_id == id {
                    ensure!(request_id.is_none(), UNAVAILABLE);
                    request_id = Some(value.request_id.clone());
                }
            }
        }
        if let Some(request_id) = request_id {
            ensure!(
                entry.is_some_and(|entry| entry.event_id == *id
                    && entry.mode == RemovalEnvelopeMode::Guarded
                    && entry.request_id.as_deref() == Some(request_id.as_str())),
                UNAVAILABLE
            );
        }
        Ok(())
    }
}
