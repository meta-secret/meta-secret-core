use crate::models::{UserSignature, VaultDoc};
use crate::node::db::events::kv_log_event::{KvKey, KvLogEvent};
use crate::node::db::events::object_descriptor::ObjectDescriptor;
use crate::node::db::events::object_id::{IdGen, ObjectId};

pub fn join_cluster_request(curr_obj_id: &ObjectId, user_sig: &UserSignature) -> KvLogEvent<UserSignature> {
    let key = KvKey {
        obj_id: curr_obj_id.next(),
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
    let maybe_error = None;

    if let Some(err_msg) = maybe_error {
        return KvLogEvent {
            key: request.key.next(),
            value: serde_json::from_str(err_msg).unwrap(),
        };
    }

    let user_sig: UserSignature = request.value.clone();

    let mut new_vault = vault.clone();
    new_vault.signatures.push(user_sig);

    KvLogEvent {
        key: request.key.next(),
        value: new_vault,
    }
}
