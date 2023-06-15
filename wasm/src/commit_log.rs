use serde::{Deserialize, Serialize};

pub mod indexed_db {
    use async_trait::async_trait;
    use meta_secret_core::node::db::generic_db::FindAllQuery;
    use meta_secret_core::node::db::generic_db::SaveCommand;
    use meta_secret_core::node::db::models::KvLogEvent;

    use crate::db::WasmDbError;
    use crate::{idbFindAll, idbSave};
    use crate::security::ToJsValue;

    pub struct CommitLogWasmRepo {
        pub db_name: String,
        pub store_name: String,
    }

    #[async_trait(? Send)]
    impl FindAllQuery<KvLogEvent> for CommitLogWasmRepo {
        type Error = WasmDbError;

        async fn find_all(&self) -> Result<Vec<KvLogEvent>, Self::Error> {
            let events_js = idbFindAll(self.db_name.as_str(), self.store_name.as_str()).await;
            let events: Vec<KvLogEvent> = serde_wasm_bindgen::from_value(events_js)?;
            Ok(events)
        }
    }

    #[async_trait(? Send)]
    impl SaveCommand for CommitLogWasmRepo {
        type Error = WasmDbError;

        async fn save(&self, event: &KvLogEvent) -> Result<(), Self::Error> {
            let event_js = event.to_js()?;
            idbSave(
                self.db_name.as_str(),
                self.store_name.as_str(),
                event.key.key_id.obj_id.id.as_str(),
                event_js,
            )
            .await;
            Ok(())
        }
    }
}
