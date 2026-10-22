//! Native Java task ownership. This path never constructs a Bedrock NetworkHandle.
use java_protocol::session::{self, Envelope, Options};
use std::{future::Future, sync::OnceLock};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .thread_name("java-session")
            .build()
            .expect("Java session executor")
    })
}
/// A bounded event FIFO and terminal result. Dropping aborts preparation, refresh and transport.
pub struct Session {
    events: mpsc::Receiver<Envelope>,
    result: oneshot::Receiver<Result<(), String>>,
    task: JoinHandle<()>,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JavaSession")
            .field("finished", &self.task.is_finished())
            .finish()
    }
}
impl Session {
    pub fn spawn(options: Options) -> Self {
        Self::prepare(async { Ok(options) })
    }
    pub fn prepare(
        options: impl Future<Output = Result<Options, String>> + Send + 'static,
    ) -> Self {
        Self::run(move |tx| async move {
            match options.await {
                Ok(options) => session::run(options, tx, Default::default())
                    .await
                    .map_err(|e| e.to_string()),
                Err(error) => Err(error),
            }
        })
    }
    /// Owns a connection runner and all of its authorization/cancellation supervision.
    pub fn run<F, Fut>(runner: F) -> Self
    where
        F: FnOnce(mpsc::Sender<Envelope>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let (tx, events) = mpsc::channel(32);
        let (done, result) = oneshot::channel();
        let task = runtime().spawn(async move {
            let outcome = runner(tx).await;
            let _ = done.send(outcome);
        });
        Self {
            events,
            result,
            task,
        }
    }
    pub async fn next(&mut self) -> Option<Envelope> {
        self.events.recv().await
    }
    pub async fn shutdown(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }
    /// Waits for the terminal result after the event FIFO closes, avoiding a close/send race.
    pub async fn outcome(&mut self) -> Result<(), String> {
        (&mut self.result)
            .await
            .unwrap_or_else(|_| Err("Java session task terminated".to_owned()))
    }
    pub fn poll(&mut self) -> Option<Envelope> {
        self.events.try_recv().ok()
    }
    pub fn result(&mut self) -> Option<Result<(), String>> {
        self.result.try_recv().ok()
    }
    pub fn is_finished(&self) -> bool {
        self.task.is_finished()
    }
    pub fn cancel(&self) {
        self.task.abort();
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn terminal_error_survives_event_channel_close() {
        let mut session = Session::prepare(async { Err("fixture preparation failure".into()) });
        assert!(session.next().await.is_none());
        assert_eq!(
            session.outcome().await.unwrap_err(),
            "fixture preparation failure"
        );
    }
    #[tokio::test]
    async fn retiring_preparation_drops_its_owned_resources() {
        struct Guard(Option<oneshot::Sender<()>>);
        impl Drop for Guard {
            fn drop(&mut self) {
                if let Some(done) = self.0.take() {
                    let _ = done.send(());
                }
            }
        }
        let (started, ready) = oneshot::channel();
        let (done, dropped) = oneshot::channel();
        let session = Session::prepare(async move {
            let _guard = Guard(Some(done));
            let _ = started.send(());
            std::future::pending::<Result<Options, String>>().await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        session.shutdown().await;
        tokio::time::timeout(std::time::Duration::from_secs(5), dropped)
            .await
            .unwrap()
            .unwrap();
    }
}
