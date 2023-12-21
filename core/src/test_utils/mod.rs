#[cfg(test)]
pub mod meta_test_utils {
    use std::collections::HashMap;

    use tracing::info;

    use crate::crypto::keys::KeyManager;
    use crate::models::{DeviceInfo, MetaPasswordId};
    use crate::node::app_models::UserCredentials;
    use crate::node::common::model::vault::VaultName;
    use crate::node::db::events::common::{MemPoolObject, MetaPassObject, ObjectCreator};
    use crate::node::db::events::db_tail::ObjectIdDbEvent;
    use crate::node::db::events::generic_log_event::GenericKvLogEvent;
    use crate::node::db::events::local::KvLogEventLocal;
    use crate::node::db::events::object_descriptor::ObjectDescriptor;
    use crate::node::db::events::object_id::{IdGen, ObjectId};
    use crate::node::db::events::vault_event::VaultObject;

    pub fn build_meta_pass_1() -> MetaPasswordId {
        MetaPasswordId {
            id: String::from("test_pass_id_123"),
            salt: String::from("pass_salt"),
            name: String::from("test_pass"),
        }
    }

    fn build_device_a() -> DeviceInfo {
        DeviceInfo {
            device_id: String::from("a"),
            device_name: String::from("a"),
        }
    }

    fn build_device_b() -> DeviceInfo {
        DeviceInfo {
            device_id: String::from("b"),
            device_name: String::from("b"),
        }
    }

    pub fn build_user_creds_a(vault_name: &str) -> UserCredentials {
        let s_box = KeyManager::generate_secret_box(vault_name.to_string());
        let user_sig = s_box.get_user_sig(&build_device_a());

        UserCredentials {
            security_box: s_box,
            user_sig: user_sig.clone(),
        }
    }

    pub fn build_user_creds_b(vault_name: &str) -> UserCredentials {
        let s_box = KeyManager::generate_secret_box(vault_name.to_string());
        let user_sig = s_box.get_user_sig(&build_device_b());

        UserCredentials {
            security_box: s_box,
            user_sig,
        }
    }

    pub struct MetaAppTestVerifier {
        pub vault_name: VaultName,
        pub events: HashMap<ObjectId, GenericKvLogEvent>,
    }

    impl MetaAppTestVerifier {
        pub fn device_verification(&self) {
            info!("Virtual Device verification");

            for evt in self.events.values() {
                println!("{:?}", evt);
                println!();
            }

            assert_eq!(13, self.events.len());
            self.common_verification();
        }

        pub fn server_verification(&self) {
            info!("Server verification");

            assert_eq!(13, self.events.len());
            self.common_verification();
        }

        pub fn client_verification(&self) {
            info!("Client verification");

            assert_eq!(17, self.events.len());

            self.common_verification();

            self.verify_db_tail();
            self.verify_mem_pool();

            //todo!("check shared secret record")
            //self.verify_local_secret_share();
        }

        pub fn common_verification(&self) {
            self.verify_meta_vault();
            self.verify_user_creds();

            self.verify_global_index();
            self.verify_meta_pass();
            self.verify_vault();

            self.verify_distributed_meta_pass();
        }

        pub fn verify_vault(&self) {
            let vault_unit = ObjectId::unit(&ObjectDescriptor::Vault {
                vault_name: self.vault_name.clone(),
            });
            let vault_genesis = vault_unit.next();
            let vault_sign_up_update = vault_genesis.next();
            let vault_join_request = vault_sign_up_update.next();
            let vault_join_update = vault_join_request.next();

            assert!(self.events.contains_key(&vault_unit));
            assert!(self.events.contains_key(&vault_genesis));
            assert!(self.events.contains_key(&vault_sign_up_update));
            assert!(self.events.contains_key(&vault_join_request));
            assert!(self.events.contains_key(&vault_join_update));

            let join_event = self.events.get(&vault_join_update).unwrap();
            if let GenericKvLogEvent::Vault(VaultObject::JoinUpdate { event }) = join_event {
                assert_eq!(2, event.value.signatures.len());
                assert_eq!(
                    String::from("virtual-device"),
                    event.value.signatures[0].vault.device.device_name
                );
                assert_eq!(
                    String::from("client"),
                    event.value.signatures[1].vault.device.device_name
                );
            } else {
                panic!("Invalid vault object. Not enough signatures");
            }
        }

        fn verify_user_creds(&self) {
            let creds_unit_id = ObjectId::unit(&ObjectDescriptor::CredsIndex);
            assert!(self.events.contains_key(&creds_unit_id));
        }

        fn verify_meta_pass(&self) {
            let meta_pass_unit = ObjectId::unit(&ObjectDescriptor::MetaPassword {
                vault_name: self.vault_name.clone(),
            });
            assert!(self.events.contains_key(&meta_pass_unit));
            assert!(self.events.contains_key(&meta_pass_unit.next()));
        }

        fn verify_distributed_meta_pass(&self) {
            let meta_pass_genesis = ObjectId::genesis(&ObjectDescriptor::MetaPassword {
                vault_name: self.vault_name.clone(),
            });

            assert!(self.events.contains_key(&meta_pass_genesis.next()));
        }

        fn verify_global_index(&self) {
            let gi_unit = ObjectId::unit(&ObjectDescriptor::GlobalIndex);
            let gi_genesis = gi_unit.next();
            let gi_vault_record = gi_genesis.next();

            assert!(self.events.contains_key(&gi_unit));
            assert!(self.events.contains_key(&gi_genesis));
            assert!(self.events.contains_key(&gi_vault_record));
        }

        fn verify_meta_vault(&self) {
            let meta_vault_unit_id = ObjectId::unit(&ObjectDescriptor::MetaVault);
            assert!(self.events.contains_key(&meta_vault_unit_id));
        }

        fn verify_db_tail(&self) {
            let db_tail_unit = ObjectId::unit(&ObjectDescriptor::DbTail);
            assert!(self.events.contains_key(&db_tail_unit));

            let db_tail_event = self.events.get(&db_tail_unit).unwrap();
            if let GenericKvLogEvent::LocalEvent(KvLogEventLocal::DbTail { event }) = db_tail_event {
                if let Some(ObjectId::Artifact { unit_id, id, prev_id }) = &event.value.maybe_global_index_id {
                    assert_eq!(String::from("GlobalIndex:index::0"), unit_id.clone());
                    assert_eq!(String::from("GlobalIndex:index::1"), prev_id.clone());
                    assert_eq!(String::from("GlobalIndex:index::2"), id.clone());
                } else {
                    panic!("Invalid Global Index Event");
                }

                if let ObjectIdDbEvent::Id { tail_id } = &event.value.meta_pass_id {
                    let meta_pass_genesis_id = ObjectId::genesis(&ObjectDescriptor::MetaPassword {
                        vault_name: String::from("q"),
                    });
                    assert_eq!(meta_pass_genesis_id.next(), tail_id.clone());
                } else {
                    panic!("Invalid Meta Pass Id");
                }

                //TODO add verification for vault_id, maybe_mem_pool_id
            } else {
                panic!("Invalid DbTail event");
            }
        }
        fn verify_mem_pool(&self) {
            let mem_pool_unit_id = ObjectId::mempool_unit();
            let mem_pool_event = &self.events[&mem_pool_unit_id];
            if let GenericKvLogEvent::MemPool(MemPoolObject::JoinRequest { event }) = mem_pool_event {
                assert_eq!(String::from("client"), event.value.vault.device.device_name);
            } else {
                panic!("Invalid mem pool event");
            }
        }
        fn verify_local_secret_share(&self) {
            let meta_pass_genesis = ObjectId::genesis(&ObjectDescriptor::MetaPassword {
                vault_name: self.vault_name.clone(),
            });
            let meta_pass_record_id = meta_pass_genesis.next();

            let meta_pass_generic_evt = self.events.get(&meta_pass_record_id).unwrap();

            let GenericKvLogEvent::MetaPass(MetaPassObject::Update { event }) = meta_pass_generic_evt else {
                panic!("Invalid event");
            };

            /*let local_secret_share_id = ObjectId::unit(&ObjectDescriptor::SharedSecret(SharedSecretDescriptor::Split {
                vault_name: "".to_string(),
                meta_pass_id: MetaPasswordId {},
                receiver: event.value.clone(),
            });

            assert!(self.events.contains_key(&local_secret_share_id));*/
        }
    }
}
