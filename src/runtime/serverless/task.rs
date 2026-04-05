use std::net::SocketAddr;

use tokio::{io::AsyncWriteExt, net::TcpListener, sync::oneshot, task::JoinHandle};

use crate::runtime::{
    Pod, PodTrigger, WorkerTrigger,
    serverless::{
        core::Serverless,
        trigger::{ServerlessRx, ServerlessTrigger},
    },
};

pub(super) async fn serverless_task(
    mut serverless: Serverless,
    mut rx: ServerlessRx,
    addr: SocketAddr,
) {
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
        tracing::info!("failed to create tcp listener, exiting");
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
                            ServerlessTrigger::ToPod { id, trigger } => {
                                unimplemented!()
                            }
                        }
                    },
                    None => break, // sender dropped, shut down
                }
            },

            Ok((mut stream, _)) = listener.accept() => {
                let pod = serverless.get_pod(0).unwrap();
                let (reply, _) = oneshot::channel();

                let _ = pod.trigger(PodTrigger::ToWorker { id: 0, trigger: WorkerTrigger::Http { reply } }).await;


                let (_, mut writer) = stream.split();
                writer.write(b"hello, world!").await.ok();


                tracing::info!("got http connection!");
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
