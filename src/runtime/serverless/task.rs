use std::{net::SocketAddr, sync::Arc};

use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto,
};
use tokio::{net::TcpListener, task::JoinHandle};

use crate::runtime::{
    Pod,
    serverless::{
        app_service::{AppState, service_handler},
        core::Serverless,
        trigger::{ServerlessRx, ServerlessTrigger},
    },
};

pub(super) async fn serverless_task(
    mut serverless: Serverless,
    mut rx: ServerlessRx,
    app_state: Arc<AppState>,
    addr: SocketAddr,
) {
    serverless.code_store.check_fs().await;

    // now, we gotta start those threads
    // i know, this might be a bit not so memory efficient
    let mut handles = Vec::with_capacity(serverless.n_threads);
    for _ in 0..serverless.n_threads {
        let (pod, handle) = Pod::start(serverless.n_workers);
        serverless.pods.push(pod);
        handles.push(handle);
    }

    // cancel handling, this is super important
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let Ok(listener) = TcpListener::bind(addr).await else {
        tracing::error!("failed to create tcp listener, exiting");
        close_serverless(serverless, handles).await;
        return;
    };

    println!("======> server started at {addr}");

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                close_serverless(serverless, handles).await;
                break;
            },

            trigger_result = rx.recv() => {
                match trigger_result {
                    Some(trigger) => {
                        match trigger {
                            ServerlessTrigger::CreateWorker { task, reply } => {
                                reply.send(serverless.create_worker(task).await).ok();
                            }
                            ServerlessTrigger::ToPod { id: _, trigger: _ } => {
                                unimplemented!()
                            }

                            ServerlessTrigger::SetUniversalWorkerName { name, locator } => {
                                serverless.set_universal_worker_name(name, locator);
                            }

                            ServerlessTrigger::RemoveUniversalWorkerName { name } => {
                                serverless.remove_universal_worker_name(&name);
                            }

                            ServerlessTrigger::UploadWorkerCode { name, code, reply } => {
                                let err = serverless.upload_worker_code(name, code).await.err();
                                reply.send(err).ok();
                            }

                            ServerlessTrigger::RemoveWorkerCode { name } => {
                                serverless.remove_worker_code(&name).await;
                            }
                        }
                    },
                    None => break, // sender dropped, shut down
                }
            },

            Ok((stream, _)) = listener.accept() => {
                let io = TokioIo::new(stream);
                let app_state1 = app_state.clone();
                tokio::task::spawn(async move {
                    let app_state2 = app_state1;
                    if let Err(err) = auto::Builder::new(TokioExecutor::new())
                        .serve_connection(io, service_fn(move |req| {
                                service_handler(app_state2.clone(), req)

                        }))
                        .await
                    {
                        tracing::error!("error serving connection: {:#?}", err);
                    }
                });
            }
        }
    }
}

async fn close_serverless(mut serverless: Serverless, handles: Vec<JoinHandle<()>>) {
    tracing::info!("sending halt to all pods...");
    serverless.halt().await;

    tracing::info!("joining pods...");

    // signal pods to stop here, then join
    for handle in handles {
        handle.await.ok();
    }

    tracing::info!("exit");
}
