//! Native Java task ownership. This path never constructs a Bedrock NetworkHandle.
mod report;
pub mod telemetry;
use java_protocol::session::{self, Command, Envelope, Options};
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
    commands: mpsc::Sender<Command>,
    report: report::Report,
    telemetry: Option<std::sync::Arc<telemetry::Recorder>>,
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
        Self::run(move |tx, commands| async move {
            match options.await {
                Ok(mut options) => {
                    options.commands = Some(commands);
                    session::run(options, tx, Default::default())
                        .await
                        .map_err(|e| e.to_string())
                }
                Err(error) => Err(error),
            }
        })
    }
    /// Owns a connection runner and all of its authorization/cancellation supervision.
    pub fn run<F, Fut>(runner: F) -> Self
    where
        F: FnOnce(mpsc::Sender<Envelope>, mpsc::Receiver<Command>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), String>> + Send + 'static,
    {
        let (tx, events) = mpsc::channel(32);
        let (commands, input) = mpsc::channel(32);
        let (done, result) = oneshot::channel();
        let task = runtime().spawn(async move {
            let outcome = runner(tx, input).await;
            let _ = done.send(outcome);
        });
        let telemetry_flag = std::env::var("CINNABAR_JAVA_TELEMETRY").ok();
        let telemetry = telemetry::Recorder::configured(
            telemetry_flag
                .as_deref()
                .or(option_env!("CINNABAR_JAVA_TELEMETRY_BUILD")),
        );
        let report = report::Report::with_telemetry(telemetry.clone());
        Self {
            events,
            result,
            task,
            commands,
            report,
            telemetry,
        }
    }
    /// Absent by default; observation handles never own a connection or mutate gameplay.
    pub fn telemetry(&self) -> Option<&std::sync::Arc<telemetry::Recorder>> {
        self.telemetry.as_ref()
    }
    /// Enable bounded, payload-free attempt reports beside the app's normal logs.
    pub fn with_report_directory(mut self, directory: std::path::PathBuf) -> Self {
        if self.telemetry.is_none() {
            self.telemetry = telemetry::Recorder::from_directory(&directory);
            self.report.set_telemetry(self.telemetry.clone());
        }
        self.report.directory(directory);
        self
    }
    /// Add a static local admission/presentation warning, deduplicated across frames.
    pub fn diagnostic_notice(&mut self, notice: &'static str) -> bool {
        self.report.notice(notice)
    }
    /// End an attempt rejected by world admission, keeping the local static error in its report.
    pub fn fail_admission(&mut self, notice: &'static str) {
        self.task.abort();
        self.observe_pending();
        self.report.notice(notice);
        self.report.finish("world_admission_error", None);
    }
    fn observe_pending(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.report.observe(&event);
        }
    }
    /// Reports full/retired input queues instead of dropping required intent silently.
    pub fn command(&self, command: Command) -> Result<(), mpsc::error::TrySendError<Command>> {
        if let Some(t) = &self.telemetry {
            // Do not expose a command as queued until the bounded FIFO accepts it.
            let permit = match self.commands.try_reserve() {
                Ok(permit) => permit,
                Err(error) => {
                    use telemetry::Counter;
                    t.count(
                        match error {
                            mpsc::error::TrySendError::Full(_)
                                if matches!(&command, Command::ContainerClick { .. }) =>
                            {
                                Counter::ContainerQueueFull
                            }
                            mpsc::error::TrySendError::Full(_) => Counter::ActionQueueFull,
                            mpsc::error::TrySendError::Closed(_) => Counter::CommandClosed,
                        },
                        1,
                    );
                    return Err(match error {
                        mpsc::error::TrySendError::Full(_) => {
                            mpsc::error::TrySendError::Full(command)
                        }
                        mpsc::error::TrySendError::Closed(_) => {
                            mpsc::error::TrySendError::Closed(command)
                        }
                    });
                }
            };
            t.command(&command);
            permit.send(command);
            Ok(())
        } else {
            self.commands.try_send(command)
        }
    }
    pub async fn next(&mut self) -> Option<Envelope> {
        let event = self.events.recv().await?;
        self.report.observe(&event);
        Some(event)
    }
    pub async fn shutdown(mut self) {
        self.task.abort();
        let _ = (&mut self.task).await;
    }
    /// Waits for the terminal result after the event FIFO closes, avoiding a close/send race.
    pub async fn outcome(&mut self) -> Result<(), String> {
        let result = (&mut self.result)
            .await
            .unwrap_or_else(|_| Err("Java session task terminated".to_owned()));
        self.observe_pending();
        self.report.finish(
            if result.is_err() { "error" } else { "closed" },
            result.as_ref().err().map(String::as_str),
        );
        result
    }
    pub fn poll(&mut self) -> Option<Envelope> {
        let event = self.events.try_recv().ok()?;
        self.report.observe(&event);
        Some(event)
    }
    pub fn result(&mut self) -> Option<Result<(), String>> {
        // Preserve ordered diagnostics/admission before returning a terminal task result.
        if !self.events.is_empty() {
            return None;
        }
        let result = match self.result.try_recv() {
            Ok(result) => result,
            Err(oneshot::error::TryRecvError::Closed) => {
                Err("Java session task terminated".to_owned())
            }
            Err(oneshot::error::TryRecvError::Empty) => return None,
        };
        self.report.finish(
            if result.is_err() { "error" } else { "closed" },
            result.as_ref().err().map(String::as_str),
        );
        Some(result)
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
        self.observe_pending();
        match self.result.try_recv() {
            Ok(result) => self.report.finish(
                if result.is_err() { "error" } else { "closed" },
                result.as_ref().err().map(String::as_str),
            ),
            Err(_) => self.report.finish("cancelled_or_retired", None),
        }
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
