use std::{net::SocketAddr, sync::Arc};

use salvo::{affix_state, catcher::Catcher, prelude::*};
use serde_json::json;
use tokio::io;

use crate::runtime::serverless::{app_security::AuthMiddleware, handle::ServerlessHandle};

struct AppState {
    serverless: ServerlessHandle,
}

pub(super) async fn start_server(
    addr: SocketAddr,
    serverless: ServerlessHandle,
    secret: String,
) -> io::Result<()> {
    let listener = TcpListener::new(addr).bind().await;

    let router = Router::new()
        .hoop(affix_state::inject(Arc::new(AppState { serverless })))
        .push(
            Router::with_path("/_/create/{name}")
                .hoop(AuthMiddleware::new(secret))
                .post(api_create_worker),
        )
        .push(Router::with_path("/worker/{name}/{**rest}").get(worker))
        .push(Router::with_path("{**}").goal(wildcard));

    println!("=====> server started at {}", addr);
    Server::new(listener)
        .serve(Service::new(router).catcher(Catcher::default().hoop(handle_error)))
        .await;
    Ok(())
}

#[handler]
async fn handle_error(res: &mut Response) {
    let status = res.status_code.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({
        "error": status.canonical_reason().unwrap_or("unknown"),
    })));
}

#[handler]
async fn wildcard() -> &'static str {
    "{}"
}

#[handler]
async fn worker(req: &mut Request, res: &mut Response, depot: &Depot) {
    let name = req.param::<String>("name").unwrap();
    let state = depot.obtain::<Arc<AppState>>().unwrap();

    let Some((pod, wrk)) = state.serverless.create_worker(name).await else {
        res.render(errored("failed to create worker"));
        return;
    };

    let Some(result) = state.serverless.send_http_to_worker(pod, wrk).await else {
        res.render(errored("failed to execute worker"));
        return;
    };
    res.render(result);
}

#[handler]
async fn api_create_worker(req: &mut Request, res: &mut Response, depot: &Depot) {
    let worker_name = req.param::<String>("name").unwrap();
    let worker_bytes = match req.payload().await {
        Ok(t) => t,
        Err(err) => {
            tracing::error!("failed to parse body, reason: {:?}", err);
            res.render(errored("failed to parse body"));
            return;
        }
    }
    .clone(); // super cheap!

    let state = depot.obtain::<Arc<AppState>>().unwrap();

    let result = state
        .serverless
        .upload_worker(worker_name, worker_bytes)
        .await;

    if let Some(err) = result {
        res.render(errored(err.to_string()));
    } else {
        res.render(Json(json!({"ok": true})));
    }
}

#[inline(always)]
fn errored<K: serde::Serialize>(s: K) -> Json<serde_json::Value> {
    Json(json!({"ok": false, "error": s}))
}
