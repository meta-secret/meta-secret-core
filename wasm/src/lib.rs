extern crate core;

use meta_secret_core::recover_from_shares;
use meta_secret_core::secret::data_block::common::SharedSecretConfig;
use meta_secret_core::secret::shared_secret::{PlainText, SharedSecretEncryption, UserShareDto};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use wasm_bindgen::prelude::*;

pub mod app_state_manager;
pub mod objects;
pub mod utils;
pub mod wasm_app_state_manager;
pub mod wasm_repo;

use tracing_subscriber::fmt::format::Pretty;
use tracing_subscriber::fmt::time::UtcTime;
use tracing_subscriber::prelude::*;
use tracing_web::{performance_layer, MakeConsoleWriter};

/// Json utilities https://github.com/rustwasm/wasm-bindgen/blob/main/crates/js-sys/tests/wasm/JSON.rs

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen]
    pub async fn updateJsState(app_state: JsValue);
}

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn debug(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn info(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn warn(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn error(s: &str);
}

#[wasm_bindgen]
extern "C" {
    pub async fn idbGet(db_name: String, store_name: String, key: String) -> JsValue;
    pub async fn idbSave(db_name: String, store_name: String, key: String, value: JsValue);

    pub async fn idbDelete(db_name: &str, store_name: &str, key: &str);

    pub async fn idbFindAll(db_name: &str, store_name: &str) -> JsValue;
}

#[wasm_bindgen]
pub fn configure() {
    utils::set_panic_hook();

    let fmt_layer = tracing_subscriber::fmt::layer()
        .without_time()
        .with_ansi(false)
        //.with_max_level(Level::INFO)
        .pretty() // Only partially supported across browsers
        .with_timer(UtcTime::rfc_3339()) // std::time is not available in browsers
        .with_writer(MakeConsoleWriter); // write events to the console

    let perf_layer = performance_layer().with_details_from_fields(Pretty::default());

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(perf_layer)
        .init();
}

/// https://rustwasm.github.io/docs/wasm-bindgen/reference/arbitrary-data-with-serde.html
#[wasm_bindgen]
pub fn split(pass: &str) -> Result<JsValue, JsValue> {
    let plain_text = PlainText::from(pass);
    let config = SharedSecretConfig {
        number_of_shares: 3,
        threshold: 2,
    };
    let shared_secret = SharedSecretEncryption::new(config, &plain_text).map_err(JsError::from)?;

    let mut res: Vec<UserShareDto> = vec![];
    for share_index in 0..config.number_of_shares {
        let share: UserShareDto = shared_secret.get_share(share_index);
        res.push(share);
    }

    let shares_js = serde_wasm_bindgen::to_value(&res)?;
    Ok(shares_js)
}

#[wasm_bindgen]
pub fn restore_password(shares_json: JsValue) -> Result<JsValue, JsValue> {
    info("wasm: restore password, core functionality");

    let user_shares: Vec<UserShareDto> = serde_wasm_bindgen::from_value(shares_json)?;

    let plain_text = recover_from_shares(user_shares).map_err(JsError::from)?;
    Ok(JsValue::from_str(plain_text.text.as_str()))
}
