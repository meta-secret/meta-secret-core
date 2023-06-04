use rand::RngCore;
use worker::*;
use serde::{Serialize, Deserialize};
use rand::rngs::OsRng as RandOsRng;
use meta_secret_core::crypto::utils;


#[event(fetch)]
async fn main(req: Request, env: Env, ctx: Context) -> Result<Response> {
    #[derive(Deserialize, Serialize)]
    struct Account {
        id: u64,
    }

    let router = Router::new();

    router
        .get_async("/sync", |_req, ctx| async move {

            match ctx.kv("yay") {
                Ok(kv) => {
                    for _i in 0..1500 {
                        let mut rnd_arr: [u8; 32] = [0; 32];
                        let mut cs_prng = RandOsRng {};
                        cs_prng.fill_bytes(&mut rnd_arr);
                        let rnd_val = rnd_arr.to_vec();

                        let hash_str = utils::generate_hash();
                        let hash = hash_str.as_str();
                        kv.put(hash, rnd_val).unwrap().execute().await.unwrap();
                        let val = kv.get(hash).text().await.unwrap().unwrap();
                    }

                    Response::from_json(&String::from("ok"))
                }
                Err(err) => {
                    Response::from_json(&err.to_string())
                }
            }
        })
        .post_async("/send", |_req, ctx| async move {
            let account = Account {
                id: 123,
            };
            Response::from_json(&account)
        })
        .run(req, env)
        .await
}