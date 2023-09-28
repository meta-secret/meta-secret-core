use std::future::Future;

use async_trait::async_trait;

#[async_trait(? Send)]
pub trait TaskRunner {
    async fn run_async<FClosure, F>(&self, future_closure: FClosure)
    where
        FClosure: FnOnce() -> F,
        F: Future<Output = ()> + 'static;
}
