use crate::models::{UserSignature, VaultDoc};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};

pub fn join_cluster_request(vault_tail_id: &ObjectId, user_sig: &UserSignature) -> KvLogEvent<UserSignature> {
    let key = KvKey {
        obj_id: vault_tail_id.next(),
        obj_desc: ObjectDescriptor::Vault {
            vault_name: user_sig.vault.name.clone(),
        },
    };

    KvLogEvent {
        key,
        value: user_sig.clone(),
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
