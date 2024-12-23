use serde::Serialize;
use wasm_bindgen::prelude::*;

pub trait ToJsValue {
    fn to_js(&self) -> Result<JsValue, JsValue>;
}

impl<T: Serialize> ToJsValue for T {
    fn to_js(&self) -> Result<JsValue, JsValue> {
        let js_value: JsValue = serde_wasm_bindgen::to_value(self)?;
        Ok(js_value)
    }
}
