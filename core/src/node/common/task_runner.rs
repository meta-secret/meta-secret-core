use std::future::Future;
use async_trait::async_trait;

#[async_trait(? Send)]
pub trait TaskRunner{
    async fn spawn(&self, future: impl Future<Output=()> + 'static);
}

#[cfg(test)]
mod test {
    use std::future::Future;
    use std::ops::Deref;
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;
    use crate::node::common::task_runner::{TaskRunner};

    pub struct RustTaskRunner {}

    #[async_trait(? Send)]
    impl TaskRunner for RustTaskRunner {
        async fn spawn(&self, future: impl Future<Output=()> + 'static) {
            use tokio::task;
            let _ = task::spawn_local(future).await;
        }
    }

    #[tokio::test]
    async fn spawn_test() {
        let mutex = Arc::new(Mutex::new(false));
        let mutex_2 = mutex.clone();

        let runner = RustTaskRunner {};
        runner.spawn(async move {
            println!("1. Async task");

            let mut executed = mutex_2.lock().unwrap();
            *executed = true;
        }).await;

        println!("2. Main thread");

        let executed = mutex.lock().unwrap();
        let executed = executed.deref();
        assert!(executed);
    }
}