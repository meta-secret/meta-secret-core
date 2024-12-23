use async_trait::async_trait;

pub struct ServiceState<T> {
    pub app_state: T,
}

#[async_trait(? Send)]
pub trait ActionHandlerWithResult<Request, Response, State> {
    async fn handle(&self, msg: Request, ctx: &mut ServiceState<State>) -> Response;
}

#[async_trait(? Send)]
pub trait ActionHandler<Request, State> {
    async fn handle(&self, msg: Request, ctx: &mut ServiceState<State>);
}
