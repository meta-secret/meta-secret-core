#![cfg(target_arch = "wasm32")]

use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn contended_sync_guard_uses_the_wasm_safe_wait_path() {
    let mutex = async_lock::Mutex::new(());

    let lock_holder = async {
        let _guard = mutex.lock().await;
        futures_lite::future::yield_now().await;
    };
    let lock_waiter = async {
        let _guard = mutex.lock().await;
    };

    // Polling both futures together deliberately sends the waiter through the
    // contended lock path. The previous async-mutex implementation called
    // std::time::Instant::now() here and panicked in browser WASM.
    futures_lite::future::zip(lock_holder, lock_waiter).await;
}
