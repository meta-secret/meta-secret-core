use std::future::Future;

use async_trait::async_trait;

#[async_trait(? Send)]
pub trait TaskRunner {
    async fn spawn(&self, future: impl Future<Output = ()> + 'static);
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;
    use std::future::Future;
    use std::rc::Rc;

    use async_trait::async_trait;

    use crate::node::common::task_runner::TaskRunner;

    pub struct RustTaskRunner {}

    #[async_trait(? Send)]
    impl TaskRunner for RustTaskRunner {
        async fn spawn(&self, future: impl Future<Output = ()> + 'static) {
            let local = tokio::task::LocalSet::new();

            local.spawn_local(async move {
                future.await;
            });

            local.await;
        }
    }

    #[tokio::test]
    async fn spawn_test() {
        let shared_obj = Rc::new(RefCell::new(false));
        let shared_obj_2 = shared_obj.clone();

        let runner = RustTaskRunner {};
        runner
            .spawn(async move {
                println!("1. Async task");
                shared_obj.replace(true);
            })
            .await;

        println!("2. Main thread");

        let executed = shared_obj_2.take();
        assert!(executed);
    }
}
