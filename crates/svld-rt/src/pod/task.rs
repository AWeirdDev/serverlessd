use tokio::sync::oneshot;

use crate::{
    WorkerTrigger,
    pod::{PodTrigger, core::Pod, trigger::PodRx},
};

#[tracing::instrument(name = "pod_task", skip_all)]
pub(super) async fn pod_task(mut pod: Pod, mut rx: PodRx) {
    while let Some(event) = rx.recv().await {
        match event {
            PodTrigger::CheckVacancies { reply } => {
                tracing::info!("pod checking vacancies, workers: {:?}", pod.workers);
                reply.send(pod.has_vacancies()).ok();
            }

            PodTrigger::WarmUpWorker { reply } => {
                let id = unsafe { pod.create_and_warmup_worker().unwrap_unchecked() };
                reply.send(id).ok();
            }

            PodTrigger::ToWorker { id, trigger } => {
                if let Some(worker) = pod.get_worker(id) {
                    let _ = worker.trigger(trigger).await;
                }
            }

            PodTrigger::Kill { token } => {
                tracing::info!("killing workers in this pod...");
                for worker in pod.workers.drain(..) {
                    if let Some(worker) = worker.get_handle() {
                        let (wtoken, recv) = oneshot::channel();
                        let _ = worker.trigger(WorkerTrigger::HaltTask).await;
                        let _ = worker.trigger(WorkerTrigger::Kill { token: wtoken }).await;
                        recv.await.ok();
                    }
                }

                tracing::info!("all workers closed in this pod");
                token.send(()).ok();
                break;
            }

            PodTrigger::MarkWorkerAsSleeping { id } => {
                tracing::info!("mark worker {id} as sleeping");
                let _ = pod.mark_worker_as_sleeping(id);
            }

            PodTrigger::RemoveWorker { id } => {
                if !pod.remove_worker(id) {
                    tracing::warn!(
                        "failed to remove worker of id: {}, probably already slept",
                        id
                    );
                }
            }
        }
    }
}
