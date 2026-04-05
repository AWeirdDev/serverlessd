use tokio::sync::oneshot;

use crate::runtime::{
    WorkerTrigger,
    pod::{PodTrigger, core::Pod, trigger::PodRx},
};

#[tracing::instrument(name = "pod_task", skip_all)]
pub(super) async fn pod_task(mut pod: Pod, mut rx: PodRx) {
    while let Some(event) = rx.recv().await {
        match event {
            PodTrigger::CheckVacancies { reply } => {
                reply.send(pod.has_vacancy()).ok();
            }

            PodTrigger::CreateWorker { task, reply } => {
                let id = pod.create_worker(task);
                reply.send(id).ok();
            }

            PodTrigger::ToWorker { id, trigger } => {
                if let Some(worker) = pod.get_worker(id) {
                    let _ = worker.trigger(trigger).await;
                }
            }

            PodTrigger::Halt { token } => {
                for worker in pod.workers.drain(..) {
                    tracing::info!("closing workers in this pod...");

                    let (wtoken, recv) = oneshot::channel();

                    if let Some(worker) = worker {
                        let _ = worker.trigger(WorkerTrigger::Halt { token: wtoken }).await;
                    }

                    recv.await.ok();
                }

                tracing::info!("all workers closed in this pod");
                token.send(()).ok();
                break;
            }
        }
    }
}
