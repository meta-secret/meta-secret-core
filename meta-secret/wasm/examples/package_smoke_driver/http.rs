//! Bounded standard HTTP/1 transport; domain work stays with one ServerApp owner.
use super::{ContextState, Http};
use anyhow::{Result, ensure};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use hyper::server::conn::http1;
use hyper_util::{
    rt::{TokioIo, TokioTimer},
    service::TowerToHyperService,
};
use std::{net::TcpListener as StdListener, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::watch, task::JoinSet};

const LIMIT: usize = 1024 * 1024;
const IO_DEADLINE: Duration = Duration::from_secs(10);
#[derive(Clone)]
struct RouteState {
    ctx: Arc<ContextState>,
    control: bool,
}
fn response(status: u16, value: serde_json::Value, cors: Option<&str>) -> Response {
    let mut result = (StatusCode::from_u16(status).unwrap(), Json(value)).into_response();
    result
        .headers_mut()
        .insert("connection", HeaderValue::from_static("close"));
    if let Some(origin) = cors {
        result.headers_mut().insert(
            "access-control-allow-origin",
            HeaderValue::from_str(origin).unwrap(),
        );
        result
            .headers_mut()
            .insert("vary", HeaderValue::from_static("Origin"));
        result.headers_mut().insert(
            "access-control-allow-headers",
            HeaderValue::from_static("Content-Type, Access-Control-Allow-Origin"),
        );
        result.headers_mut().insert(
            "access-control-allow-methods",
            HeaderValue::from_static("POST, OPTIONS"),
        );
    }
    result
}
async fn dispatch(State(route): State<RouteState>, request: Request) -> Response {
    let cors = (!route.control).then_some(route.ctx.shared.origin.as_str());
    let failure = || {
        response(
            400,
            serde_json::json!({"errorCode": if route.control { "ControlUnavailable" } else { "DriverUnavailable" }}),
            cors,
        )
    };
    let (parts, body) = request.into_parts();
    let origin = match parts.headers.get("origin").map(|v| v.to_str()).transpose() {
        Ok(v) => v.map(str::to_owned),
        Err(_) => return failure(),
    };
    let mut stop = route.ctx.shared.stop.subscribe();
    if *stop.borrow() {
        return failure();
    }
    let bytes = tokio::select! {
        _ = stop.changed() => return failure(),
        read = tokio::time::timeout(IO_DEADLINE, to_bytes(body, LIMIT)) => match read {
            Ok(Ok(bytes)) => bytes, _ => return failure(),
        }
    };
    if *stop.borrow() {
        return failure();
    }
    let http = Http {
        method: parts.method.to_string(),
        path: parts
            .uri
            .path_and_query()
            .map_or("/", |v| v.as_str())
            .to_owned(),
        origin,
        body: bytes.to_vec(),
    };
    let result = if route.control {
        super::control(http, &route.ctx.shared)
    } else {
        super::data(http, &route.ctx).await
    };
    match result {
        Ok((status, value)) => response(status, value, cors),
        Err(_) => failure(),
    }
}
async fn listen(
    listener: TcpListener,
    router: Router,
    mut stop: watch::Receiver<bool>,
    cap: usize,
    ctx: Arc<ContextState>,
) -> Result<()> {
    let mut tasks = JoinSet::new();
    let mut failed = false;
    loop {
        if *stop.borrow() {
            break;
        }
        tokio::select! {
            _ = stop.changed() => break,
            done = tasks.join_next(), if !tasks.is_empty() => {
                if done.is_some_and(|r| !matches!(r, Ok(false))) { failed = true; let _ = ctx.shared.stop.send(true); break; }
            }
            accepted = listener.accept(), if tasks.len() < cap => {
                let socket = match accepted { Ok((socket, _)) => socket, Err(_) => { failed = true; let _ = ctx.shared.stop.send(true); break; } };
                let routed = TowerToHyperService::new(router.clone());
                let (ready, mut response_ready) = watch::channel(false);
                let service = hyper::service::service_fn(move |request| {
                    let routed = routed.clone();
                    let ready = ready.clone();
                    async move {
                        let response = hyper::service::Service::call(&routed, request).await;
                        if response.is_ok() { let _ = ready.send(true); }
                        response
                    }
                });
                let mut shutdown = stop.clone();
                tasks.spawn(async move {
                    if *shutdown.borrow() { return false; }
                    let mut builder = http1::Builder::new();
                    builder.timer(TokioTimer::new()).header_read_timeout(IO_DEADLINE).max_buf_size(LIMIT).keep_alive(false);
                    let connection = builder.serve_connection(TokioIo::new(socket), service);
                    tokio::pin!(connection);
                    let write_limit = async move {
                        while !*response_ready.borrow_and_update() {
                            if response_ready.changed().await.is_err() {
                                std::future::pending::<()>().await;
                            }
                        }
                        // Start only after Router returned the actual Response.
                        tokio::time::sleep(IO_DEADLINE).await;
                    };
                    tokio::pin!(write_limit);
                    let mut draining = false;
                    loop {
                        tokio::select! {
                            _ = &mut connection => break false,
                            _ = &mut write_limit => break true,
                            _ = shutdown.changed(), if !draining => {
                                draining = true;
                                connection.as_mut().graceful_shutdown();
                            }
                        }
                    }
                    // A write timeout is a driver failure; ordinary peer closure is local.
                });
            }
        }
    }
    drop(listener);
    while let Some(done) = tasks.join_next().await {
        if !matches!(done, Ok(false)) {
            failed = true;
        }
    }
    ensure!(!failed, "G4.HttpTaskUnavailable");
    Ok(())
}
pub(super) async fn run(
    data: StdListener,
    control: StdListener,
    ctx: Arc<ContextState>,
) -> Result<()> {
    let data = TcpListener::from_std(data)?;
    let control = TcpListener::from_std(control)?;
    let data_router = Router::new().fallback(dispatch).with_state(RouteState {
        ctx: ctx.clone(),
        control: false,
    });
    let control_router = Router::new().fallback(dispatch).with_state(RouteState {
        ctx: ctx.clone(),
        control: true,
    });
    let signal = ctx.shared.stop.clone();
    let deadline = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(600)).await;
        let _ = signal.send(true);
    });
    let (data_result, control_result) = tokio::join!(
        listen(
            data,
            data_router,
            ctx.shared.stop.subscribe(),
            16,
            ctx.clone()
        ),
        listen(
            control,
            control_router,
            ctx.shared.stop.subscribe(),
            4,
            ctx.clone()
        ),
    );
    deadline.abort();
    let _ = deadline.await;
    // Both listeners, routers and all connection tasks are gone before dropping ctx.
    data_result?;
    control_result?;
    Ok(())
}
