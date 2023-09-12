use std::future::Future;
use async_trait::async_trait;

#[async_trait]
pub trait TaskRunner{
    async fn spawn<F>(&self, future: F) where F: Future<Output=()> + Send + 'static;
}

pub struct RustTaskRunner {}

#[async_trait]
impl TaskRunner for RustTaskRunner {
    async fn spawn<F>(&self, future: F) where F: Future<Output=()> + Send + 'static {
        use async_std::task;
        task::spawn(async move {
            future.await;
        }).await;
    }
}

#[cfg(test)]
mod test {
    use std::ops::Deref;
    use std::sync::{Arc, Mutex};

    use crate::node::common::task_runner::{RustTaskRunner, TaskRunner};

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