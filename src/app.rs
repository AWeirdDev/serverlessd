use std::{net::SocketAddr, sync::Arc};

use askama::Template;
use salvo::{
    affix_state,
    catcher::Catcher,
    http::{HeaderName, HeaderValue},
    prelude::*,
};
use serde_json::json;
use tokio::io;

use svld_rt::{CreateWorkerError, ServerlessHandle};

use crate::app_security::AuthMiddleware;

#[derive(Template)]
#[template(path = "404.html")]
struct NotFoundTemplate;

#[derive(Template)]
#[template(path = "error.html")]
struct ErrorTemplate<'a> {
    reasoning: &'a str,
}

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
            Router::new()
                .hoop(AuthMiddleware::new(secret))
                .push(Router::with_path("/_/upload/{name}").post(api_upload_worker))
                .push(Router::with_path("/_/remove/{name}").post(api_remove_worker)),
        )
        .push(Router::with_path("/worker/{name}/{**rest}").get(worker))
        .push(Router::with_path("{**}").goal(wildcard));

    println!("=====> server started at http://{}", addr);
    Server::new(listener)
        .serve(Service::new(router).catcher(Catcher::default().hoop(handle_error)))
        .await;
    Ok(())
}

#[handler]
async fn handle_error(res: &mut Response) {
    let status = res.status_code.unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    res.render(Json(json!({
        "ok": false,
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

    let (pod, wrk) = match state.serverless.create_worker(name).await {
        Ok(t) => t,
        Err(err) => {
            match err {
                CreateWorkerError::UnknownWorker(_) => {
                    res.status_code(StatusCode::NOT_FOUND);
                    res.add_header(
                        HeaderName::from_static("content-type"),
                        HeaderValue::from_static("text/html"),
                        true,
                    )
                    .ok();
                    res.render(NotFoundTemplate.to_string());
                }
                CreateWorkerError::CannotCreateTask => {
                    res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    res.add_header(
                        HeaderName::from_static("content-type"),
                        HeaderValue::from_static("text/html"),
                        true,
                    )
                    .ok();
                    res.render(
                        ErrorTemplate {
                            reasoning: "We couldn't allocate any space for your worker. That's fucked up.",
                        }
                        .to_string(),
                    );
                }
            }

            return;
        }
    };

    let Some(result) = state.serverless.send_http_to_worker(pod, wrk).await else {
        res.add_header(
            HeaderName::from_static("content-type"),
            HeaderValue::from_static("text/html"),
            true,
        )
        .ok();
        res.render(
            ErrorTemplate {
                reasoning: "Failed to execute worker; an unknown error occurred.",
            }
            .to_string(),
        );
        return;
    };

    match result {
        Ok(t) => {
            tracing::info!("got result {}", t);
            res.render(t);
        }
        Err(err) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.add_header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("text/html"),
                true,
            )
            .ok();
            res.render(
                ErrorTemplate {
                    reasoning: &err.to_string(),
                }
                .to_string(),
            );
        }
    }
}

#[handler]
async fn api_upload_worker(req: &mut Request, res: &mut Response, depot: &Depot) {
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

#[handler]
async fn api_remove_worker(req: &mut Request, res: &mut Response, depot: &Depot) {
    let worker_name = req.param::<String>("name").unwrap();
    let state = depot.obtain::<Arc<AppState>>().unwrap();
    state.serverless.remove_worker_code(worker_name).await;

    res.render(Json(json!({"ok": true})));
}

#[inline(always)]
fn errored<K: serde::Serialize>(s: K) -> Json<serde_json::Value> {
    Json(json!({"ok": false, "error": s}))
}
