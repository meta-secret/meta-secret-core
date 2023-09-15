use async_trait::async_trait;
use meta_secret_core::node::common::task_runner::TaskRunner;
use std::future::Future;
use wasm_bindgen_futures::spawn_local;

pub struct WasmTaskRunner {}

#[async_trait(? Send)]
impl TaskRunner for WasmTaskRunner {
    async fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        spawn_local(async move {
            future.await;
        });
    }
}

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}

#[cfg(test)]
mod test {
    use crate::utils::WasmTaskRunner;
    use meta_secret_core::node::common::task_runner::TaskRunner;
    use std::ops::Deref;
    use std::sync::{Arc, Mutex};
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    async fn spawn_test() {
        let mutex = Arc::new(Mutex::new(false));
        let mutex_2 = mutex.clone();

        let runner = WasmTaskRunner {};
        runner
            .spawn(async move {
                println!("1. Async task");

                let mut executed = mutex_2.lock().unwrap();
                *executed = true;
            })
            .await;

        println!("2. Main thread");

        let executed = mutex.lock().unwrap();
        let executed = executed.deref();
        assert!(executed);
    }
}
