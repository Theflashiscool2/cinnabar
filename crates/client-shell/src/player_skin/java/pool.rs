//! One process-wide bounded worker pool, retained across Java sessions and resource reloads.
use super::{Downloader, Texture};
use assets::registry::java::textures::Image;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

pub type Reply = mpsc::Receiver<Result<Arc<Image>, String>>;
#[derive(Debug)]
pub enum Error {
    Busy,
    Cancelled,
    Unavailable(String),
}
enum Job {
    Head {
        texture: Texture,
        cancelled: Arc<AtomicBool>,
        reply: mpsc::SyncSender<Result<Arc<Image>, String>>,
    },
    Profile {
        query: super::profiles::Query,
        cancelled: Arc<AtomicBool>,
        reply: mpsc::SyncSender<Result<super::profiles::Resolved, String>>,
    },
}
struct Pool {
    jobs: mpsc::SyncSender<Job>,
}
static POOL: OnceLock<Result<Pool, String>> = OnceLock::new();
impl Pool {
    fn new() -> Result<Self, String> {
        let (jobs, receiver) = mpsc::sync_channel::<Job>(8);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..2 {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| "could not initialize Java skin runtime")?;
            let downloader = Downloader::new()?;
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("java-skins-{index}"))
                .spawn(move || {
                    loop {
                        let job = match receiver.lock() {
                            Ok(receiver) => receiver.recv(),
                            Err(_) => return,
                        };
                        let Ok(job) = job else {
                            return;
                        };
                        match job {
                            Job::Head {
                                texture,
                                cancelled,
                                reply,
                            } => {
                                let result = if cancelled.load(Ordering::Relaxed) {
                                    Err("Java skin request cancelled".into())
                                } else {
                                    runtime
                                        .block_on(downloader.head(&texture, &cancelled))
                                        .map(Arc::new)
                                };
                                if !cancelled.load(Ordering::Relaxed) {
                                    let _ = reply.send(result);
                                }
                            }
                            Job::Profile {
                                query,
                                cancelled,
                                reply,
                            } => {
                                let result = if cancelled.load(Ordering::Relaxed) {
                                    Err("Java public profile cancelled".into())
                                } else {
                                    runtime.block_on(downloader.profile(&query, &cancelled))
                                };
                                if !cancelled.load(Ordering::Relaxed) {
                                    let _ = reply.send(result);
                                }
                            }
                        }
                    }
                })
                .map_err(|_| "could not start Java skin worker")?;
        }
        Ok(Self { jobs })
    }
}
/// Retiring the caller's cancellation flag prevents obsolete work from publishing. A full
/// queue is temporary: callers keep the native pending/default presentation and retry later.
pub fn request(texture: Texture, cancelled: Arc<AtomicBool>) -> Result<Reply, Error> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let pool = POOL
        .get_or_init(Pool::new)
        .as_ref()
        .map_err(|e| Error::Unavailable(e.clone()))?;
    let (reply, receiver) = mpsc::sync_channel(1);
    pool.jobs
        .try_send(Job::Head {
            texture,
            cancelled,
            reply,
        })
        .map_err(|e| match e {
            mpsc::TrySendError::Full(_) => Error::Busy,
            mpsc::TrySendError::Disconnected(_) => {
                Error::Unavailable("Java skin workers stopped".into())
            }
        })?;
    Ok(receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_request_is_rejected_before_initializing_transport() {
        let texture = Texture::from_url(&format!(
            "https://textures.minecraft.net/texture/{}",
            "0123456789abcdef".repeat(4)
        ))
        .unwrap();
        assert!(matches!(
            request(texture, Arc::new(AtomicBool::new(true))),
            Err(Error::Cancelled)
        ));
    }
}

/// Resolves public head metadata on the same bounded pool; no account tokens are used.
pub fn request_profile(
    query: super::profiles::Query,
    cancelled: Arc<AtomicBool>,
) -> Result<super::profiles::Reply, Error> {
    if cancelled.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    let pool = POOL
        .get_or_init(Pool::new)
        .as_ref()
        .map_err(|e| Error::Unavailable(e.clone()))?;
    let (reply, receiver) = mpsc::sync_channel(1);
    pool.jobs
        .try_send(Job::Profile {
            query,
            cancelled,
            reply,
        })
        .map_err(|e| match e {
            mpsc::TrySendError::Full(_) => Error::Busy,
            mpsc::TrySendError::Disconnected(_) => {
                Error::Unavailable("Java skin workers stopped".into())
            }
        })?;
    Ok(receiver)
}
