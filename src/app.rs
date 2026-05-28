use std::{mem, net::SocketAddr, sync::Arc};

use salvo::{
    affix_state,
    catcher::Catcher,
    http::{HeaderName, HeaderValue},
    prelude::*,
};
use serde_json::json;

use svld_configs::DeterminationStrategy;
use svld_rt::{
    models::{WorkerHttpRequest, WorkerHttpResponse},
    serverless::CreateWorkerError,
};

use crate::handle::ServerlessHandle;

struct AppState {
    serverless: ServerlessHandle,
}

pub(super) async fn start_server(
    addr: SocketAddr,
    serverless: ServerlessHandle,
) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    let listener = TcpListener::new(addr).try_bind().await?;

    let router = Router::new()
        .hoop(affix_state::inject(Arc::new(AppState { serverless })))
        .push(Router::with_path("/worker/{name}/{**rest}").get(worker))
        .push(Router::with_path("{**}").goal(worker));

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
async fn worker(req: &mut Request, resp: &mut Response, depot: &Depot) {
    let serverless = &depot.obtain::<Arc<AppState>>().unwrap().serverless;
    let name = match serverless.global_config.determination_strategy {
        DeterminationStrategy::Path => {
            let Some(name) = req.param::<String>("name") else {
                resp.status_code(StatusCode::NOT_FOUND);
                resp.render("not found");
                return;
            };

            name
        }
        DeterminationStrategy::SubdomainName => {
            let Some(value) = req.headers().get("Host") else {
                resp.status_code(StatusCode::NOT_FOUND);
                resp.render("not found");
                return;
            };

            let name = match value.to_str() {
                Ok(t) => t,
                Err(err) => {
                    tracing::error!("failed to parse 'Host' header value: {err:?}");
                    resp.status_code(StatusCode::BAD_REQUEST);
                    resp.render("bad worker name");
                    return;
                }
            };

            let Some((left, _)) = name.split_once('.') else {
                tracing::error!("failed to split 'Host' header value: {name:?}");
                resp.status_code(StatusCode::BAD_REQUEST);
                resp.render("bad worker name");
                return;
            };

            left.to_string()
        }
    };

    let worker_req = {
        let Ok(payload) = req.payload().await else {
            resp.status_code(StatusCode::BAD_REQUEST);
            resp.render("payload too large (>64KB) or failed to load payload");
            return;
        };

        WorkerHttpRequest::builder()
            .body(payload.clone())
            .headers(mem::take(req.headers_mut()))
            .method(mem::take(req.method_mut()))
            .url(format!(
                "https://serverlessd.local{}",
                req.uri()
                    .path_and_query()
                    .map(|path_and_query| path_and_query.to_string())
                    .unwrap_or_else(|| "/".to_string())
            ))
            .build()
    };

    let (pod_id, worker_id) = match serverless.create_worker_task(name).await {
        Ok(t) => t,
        Err(err) => {
            match err {
                CreateWorkerError::UnknownWorker(_) => {
                    resp.status_code(StatusCode::NOT_FOUND);
                    resp.render("not found");
                }

                CreateWorkerError::CannotCreateTask(reason) => {
                    tracing::error!("failed to create task; reason: {reason}");

                    resp.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                    resp.add_header(
                        HeaderName::from_static("content-type"),
                        HeaderValue::from_static("text/html"),
                        true,
                    )
                    .ok();
                    resp.render("We couldn't allocate any space for this worker.");
                }
            }

            return;
        }
    };

    let res = serverless
        .send_http_to_worker(pod_id, worker_id, worker_req)
        .await;

    // this is mandatory!
    {
        let res = serverless
            .halt_task_and_clear_space(pod_id, worker_id)
            .await;
        if res.is_err() {
            tracing::error!("failed to halt task after http is done");
        }
    }

    let result = match res {
        Ok(t) => t,
        Err(err) => {
            tracing::error!("failed to execute worker {}", err);
            resp.add_header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("text/html"),
                true,
            )
            .ok();

            resp.render("Failed to execute worker; an unknown error occurred.".to_string());
            return;
        }
    };

    match result {
        Ok(WorkerHttpResponse {
            status,
            headers,
            body,
        }) => {
            resp.set_headers(headers);
            resp.status_code(status);
            resp.body(body);
        }
        Err(err) => {
            tracing::error!("got error after worker execution: {err}");

            resp.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            resp.add_header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("text/html"),
                true,
            )
            .ok();
            resp.render(format!("js error:\n{}", err.to_string()));
        }
    }
}
