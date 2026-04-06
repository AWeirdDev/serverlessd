use std::convert::Infallible;
use std::sync::Arc;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request, Response, StatusCode, header};
use matchit::Router;
use serde_json::json;
use tokio::sync::oneshot;

use crate::runtime::serverless::code_store::CodeStoreError;
use crate::runtime::serverless::{handle::ServerlessHandle, trigger::ServerlessTrigger};

pub(super) struct AppState {
    pub(super) router: Router<AppPath>,
    pub(super) serverless: ServerlessHandle,
}

impl AppState {
    pub(super) fn new(serverless: ServerlessHandle) -> Self {
        let mut router = Router::new();
        router
            .insert("/_/upload/{name}", AppPath::ApiUpload)
            .expect("failed to build app router");

        router
            .insert("/{worker}", AppPath::Worker)
            .expect("failed to build app router");
        router
            .insert("/{worker}/{*segments}", AppPath::Worker)
            .expect("failed to build app router");

        Self { router, serverless }
    }
}

pub(super) enum AppPath {
    ApiUpload,
    Worker,
}

pub(super) async fn service_handler(
    state: Arc<AppState>,
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    match service_handler_inner(state, req).await {
        Ok(response) => Ok(response),
        Err(err_response) => Ok(err_response),
    }
}

pub(super) async fn service_handler_inner(
    state: Arc<AppState>,
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, Response<Full<Bytes>>> {
    let matched = state.router.at(req.uri().path());
    if let Ok(mtch) = matched {
        match mtch.value {
            // api endpoints
            AppPath::ApiUpload => {
                if !matches!(req.method(), &Method::POST) {
                    let data = serde_json::to_vec(&json!({"error": "method not alowed"})).unwrap();
                    return Ok(Response::builder()
                        .header(header::CONTENT_TYPE, "application/json")
                        .status(StatusCode::METHOD_NOT_ALLOWED)
                        .body(Full::new(Bytes::from(data)))
                        .unwrap());
                }

                let name = mtch.params.get("name").unwrap().to_string();
                if &name == "_" {
                    let data =
                        serde_json::to_vec(&json!({"error": "invalid worker name"})).unwrap();
                    return Ok(Response::builder()
                        .header(header::CONTENT_TYPE, "application/json")
                        .status(StatusCode::BAD_REQUEST)
                        .body(Full::new(Bytes::from(data)))
                        .unwrap());
                }

                let code_bytes = req.into_body().collect().await.unwrap().to_bytes();
                let code = String::from_utf8(code_bytes.to_vec()).unwrap();

                let (reply, recv) = oneshot::channel();
                state
                    .serverless
                    .trigger(ServerlessTrigger::UploadWorkerCode { name, code, reply })
                    .await;

                if let Some(err) = recv.await.unwrap() {
                    return match &err {
                        CodeStoreError::InvalidName(_) => {
                            let data =
                                serde_json::to_vec(&json!({ "error": err.to_string() })).unwrap();
                            Ok(Response::builder()
                                .header(header::CONTENT_TYPE, "application/json")
                                .status(StatusCode::BAD_REQUEST)
                                .body(Full::new(Bytes::from(data)))
                                .unwrap())
                        }
                        CodeStoreError::IoError(_) => {
                            tracing::error!("failed to upload worker (io), reason: {:?}", err);

                            let data = serde_json::to_vec(
                                &json!({ "error": "failed to upload worker, try again later" }),
                            )
                            .unwrap();
                            Ok(Response::builder()
                                .header(header::CONTENT_TYPE, "application/json")
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(Full::new(Bytes::from(data)))
                                .unwrap())
                        }
                    };
                }

                let data = serde_json::to_vec(&json!({})).unwrap();
                return Ok(Response::builder()
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Full::new(Bytes::from(data)))
                    .unwrap());
            }

            // worker (just post the request to js, fuck it)
            AppPath::Worker => {
                let segments = mtch.params.get("segments");
                let body = serde_json::to_vec(&json!({ "segments": segments })).unwrap();
                return Ok(Response::builder()
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Full::new(Bytes::from(body)))
                    .unwrap());
            }
        }
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Full::new(Bytes::from("{\"error\": \"not found\"}")))
        .unwrap())
}
