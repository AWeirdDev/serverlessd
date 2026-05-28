use std::net::SocketAddr;

use tokio::{io, task::JoinHandle};

use svld_rt::{
    pod::Pod,
    serverless::{CreateWorkerError, Serverless},
    triggers::{PodTrigger, WorkerTrigger},
    worker::WorkerTask,
};

use crate::{
    app::start_server,
    handle::ServerlessHandle,
    trigger::{ServerlessRx, ServerlessTrigger},
};

pub(super) async fn serverless_task(
    mut serverless: Serverless,
    mut rx: ServerlessRx,
    addr: SocketAddr,
    svl_handle: ServerlessHandle,
) {
    // now, we gotta start those threads
    // i know, this might be a bit not so memory efficient
    let mut handles = Vec::with_capacity(serverless.n_pods);
    for _ in 0..serverless.n_pods {
        let (pod, handle) = Pod::start(
            serverless.get_platform(),
            serverless.binding_store.clone(),
            serverless.n_workers,
            serverless.global_config.clone(),
        );
        serverless.push_pod(pod);
        handles.push(handle);
    }

    // cancel handling, this is super important
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    let api_handle = std::pin::pin!(start_server(addr, svl_handle));
    tokio::pin!(api_handle);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                close_serverless(serverless, handles).await;
                break;
            },

            result = &mut api_handle => {
                tracing::error!("server exited unexpectedly: {:?}", result);
                eprintln!("=====x error: server exited unexpectedly, exiting");
                if let Err(e) = result {
                    eprintln!("=====x error: {}", e.to_string());
                }
                break;
            },

            trigger_result = rx.recv() => {
                match trigger_result {
                    Some(trigger) => {
                        match trigger {
                            ServerlessTrigger::CreateWorkerTask { name, reply } => {
                                let worker_on_disk = match serverless.code_store.get_worker(&name).await {
                                    Some(t) => t,
                                    None => {
                                        reply.send(Err(CreateWorkerError::UnknownWorker(name))).ok();
                                        continue;
                                    }
                                };

                                tracing::info!("creating worker task");
                                let Some((pod_handle, pod_id, pod_worker_id)) = serverless.find_vacancy_and_warmup().await else {
                                    reply.send(
                                        Err(
                                            CreateWorkerError::CannotCreateTask(
                                                "failed to find vacancy and warm up worker".to_string()
                                            )
                                        )
                                    ).ok();
                                    continue;
                                };


                                    pod_handle.assign_worker_task(
                                        pod_worker_id,
                                        WorkerTask {
                                            source: worker_on_disk.code,
                                            config: worker_on_disk.config
                                        }
                                    )
                                    .await;


                                reply.send(Ok((pod_id, pod_worker_id))).ok();


                                tracing::info!("done with creating worker task");
                            }
                            ServerlessTrigger::ToPod { id, trigger } => {
                                if matches!(trigger, PodTrigger::ToWorker { id: _, trigger: WorkerTrigger::HaltTask }) {
                                    serverless.release_pod_space(id);
                                }

                                if let Some(pod) = serverless.get_pod(id) {
                                    let _ = pod.trigger(trigger).await;
                                }
                            }

                            ServerlessTrigger::UploadWorker { code, config, reply } => {
                                reply.send(serverless.upload_worker(code, config).await).ok();
                            }

                            ServerlessTrigger::RemoveWorkerCode { name } => {
                                serverless.remove_worker_code(&name).await;
                            }
                        }
                    },
                    None => break, // sender dropped, shut down
                }
            },
        }
    }
}

async fn close_serverless(mut serverless: Serverless, handles: Vec<JoinHandle<io::Result<()>>>) {
    tracing::info!("sending halt to all pods...");
    serverless.kill().await;

    tracing::info!("joining pods...");

    // signal pods to stop here, then join
    for handle in handles {
        handle.await.ok();
    }

    tracing::info!("exit");
}
