use crate::cf_kv_store::CfKvStore;
use meta_secret_core::node::db::models::KvLogEvent;
use meta_secret_core::node::server::meta_server::{MetaServer, MetaServerContextState, SyncRequest};
use serde::{Deserialize, Serialize};
use worker::*;

mod cf_kv_store;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .get_async("/sync", |mut req, ctx| async move {
            match ctx.kv("meta-secret") {
                Ok(kv) => {
                    let store = CfKvStore {
                        kv_store: kv,
                    };
                    let server = MetaServerContextState::new(store);

                    let request = req.json::<SyncRequest>().await?;

                    let log_events = server.sync(request).await;

                    Response::from_json(&log_events)
                }
                Err(err) => {
                    Response::from_json(&err.to_string())
                }
            }
        })
        .post_async("/send", |mut req, ctx| async move {
            match ctx.kv("meta-secret") {
                Ok(kv) => {
                    let store = CfKvStore {
                        kv_store: kv,
                    };
                    let server = MetaServerContextState::new(store);

                    let request = req.json::<KvLogEvent>().await?;
                    server.send(&request).await;

                    Response::from_json(&String::from("ok"))
                }
                Err(err) => {
                    Response::from_json(&err.to_string())
                }
            }
        })
        .run(req, env)
        .await
}