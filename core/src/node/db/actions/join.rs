use crate::node::common::model::user::UserDataCandidate;
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{ArtifactId, Next};

pub fn join_cluster_request(vault_tail_id: ArtifactId, user: UserDataCandidate) -> KvLogEvent<ArtifactId, UserDataCandidate> {
    let key = KvKey {
        obj_id: vault_tail_id.next(),
        obj_desc: ObjectDescriptor::Vault {
            vault_name: user.user_data.vault_name,
        },
    };

    KvLogEvent {
        key,
        value: user.clone(),
    }
}

pub fn accept_join_request(request: &KvLogEvent<UserSignature>, vault: &VaultDoc) -> KvLogEvent<VaultDoc> {
    let user_sig = request.value.clone();

    let mut new_vault = vault.clone();
    new_vault.signatures.push(user_sig);

    KvLogEvent {
        key: request.key.next(),
        value: new_vault,
    }
}
