//! Bounded public-skin cache. Callers supply monotonic time and the admitted world scope.
use super::{Texture, pool};
use assets::registry::java::{skull::skin::PIXEL_BYTES, textures::Image};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

// PlayerSkinRenderCache.CACHE_DURATION in the verified 26.3 client.
pub(super) const LIFETIME: Duration = Duration::from_secs(5 * 60);
// Explicit working-set budget; not a promise of unlimited simultaneous custom heads.
pub(super) const CAPACITY: usize = (64 * 1024 * 1024) / PIXEL_BYTES;
struct Pending {
    reply: pool::Reply,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
struct Entry {
    touched: Duration,
    state: State,
}
enum State {
    Pending(Pending),
    Ready { image: Arc<Image>, digest: [u8; 32] },
    Failed(String),
}
#[derive(Default)]
pub struct Cache {
    scope: Option<(u64, u64)>,
    entries: BTreeMap<Texture, Entry>,
}
impl Cache {
    /// Session retirement drops pending replies and cancels their work; immutable public
    /// images may be reused. No profile names, account credentials or positions are cached.
    pub fn scope(&mut self, scope: Option<(u64, u64)>) {
        if self.scope != scope {
            self.entries
                .retain(|_, e| !matches!(e.state, State::Pending(_)));
            self.scope = scope;
        }
    }
    /// Advances only the current desired resources. Saturated workers defer requests;
    /// failures remain inspectable rather than being represented as successful textures.
    pub fn update(&mut self, scope: (u64, u64), now: Duration, desired: &BTreeSet<Texture>) {
        self.scope(Some(scope));
        self.entries.retain(|key, e| {
            now.saturating_sub(e.touched) < LIFETIME
                && (!matches!(e.state, State::Pending(_)) || desired.contains(key))
        });
        for texture in desired {
            if let Some(entry) = self.entries.get_mut(texture) {
                entry.touched = now;
                if let State::Pending(p) = &entry.state {
                    entry.state = match p.reply.try_recv() {
                        Ok(Ok(image)) => State::Ready {
                            digest: Sha256::digest(&image.rgba).into(),
                            image,
                        },
                        Ok(Err(error)) => State::Failed(error),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            State::Failed("Java skin worker ended".into())
                        }
                        Err(mpsc::TryRecvError::Empty) => continue,
                    };
                }
                continue;
            }
            if self.entries.len() >= CAPACITY {
                let evict = self
                    .entries
                    .iter()
                    .filter(|(key, _)| !desired.contains(*key))
                    .min_by_key(|(_, e)| e.touched)
                    .map(|(key, _)| key.clone());
                if let Some(key) = evict {
                    self.entries.remove(&key);
                } else {
                    break;
                }
            }
            let cancelled = Arc::new(AtomicBool::new(false));
            let state = match pool::request(texture.clone(), Arc::clone(&cancelled)) {
                Ok(reply) => State::Pending(Pending { reply, cancelled }),
                Err(pool::Error::Busy) => continue,
                Err(pool::Error::Cancelled) => continue,
                Err(pool::Error::Unavailable(error)) => State::Failed(error),
            };
            self.entries.insert(
                texture.clone(),
                Entry {
                    touched: now,
                    state,
                },
            );
        }
    }
    /// Snapshots only textures wanted by this world's current authoritative profiles.
    pub fn images(&self, desired: &BTreeSet<Texture>) -> BTreeMap<String, Arc<Image>> {
        if self.scope.is_none() {
            return BTreeMap::new();
        }
        desired
            .iter()
            .filter_map(|texture| match &self.entries.get(texture)?.state {
                State::Ready { image, .. } => Some((texture.head_atlas_name(), Arc::clone(image))),
                _ => None,
            })
            .collect()
    }
    /// Hashes immutable image digests, not frame-time pixel buffers, for atlas publication.
    pub fn identity(&self, desired: &BTreeSet<Texture>) -> [u8; 32] {
        let mut hash = Sha256::new();
        if self.scope.is_some() {
            for texture in desired {
                if let Some(Entry {
                    state: State::Ready { digest, .. },
                    ..
                }) = self.entries.get(texture)
                {
                    hash.update(texture.key().as_bytes());
                    hash.update(digest);
                }
            }
        }
        hash.finalize().into()
    }
    pub fn failure(&self, texture: &Texture) -> Option<&str> {
        if !self.entries.contains_key(texture) && self.entries.len() >= CAPACITY {
            return Some("Java head image working-set budget exhausted");
        }
        match &self.entries.get(texture)?.state {
            State::Failed(e) => Some(e),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn texture(n: char) -> Texture {
        Texture::from_url(&format!(
            "https://textures.minecraft.net/texture/{}",
            n.to_string().repeat(64)
        ))
        .unwrap()
    }
    fn pending(
        cache: &mut Cache,
        t: Texture,
    ) -> (
        mpsc::SyncSender<Result<Arc<Image>, String>>,
        Arc<AtomicBool>,
    ) {
        let (tx, reply) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        cache.entries.insert(
            t,
            Entry {
                touched: Duration::ZERO,
                state: State::Pending(Pending {
                    reply,
                    cancelled: Arc::clone(&cancelled),
                }),
            },
        );
        (tx, cancelled)
    }
    #[test]
    fn session_change_cancels_and_retires_old_replies_without_publishing_them() {
        let mut cache = Cache::default();
        cache.scope(Some((1, 1)));
        let t = texture('a');
        let (tx, cancelled) = pending(&mut cache, t.clone());
        cache.scope(Some((2, 1)));
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(tx.send(Err("obsolete".into())).is_err());
        assert!(cache.images(&BTreeSet::from([t])).is_empty());
    }
    #[test]
    fn completed_image_is_immutable_and_only_published_for_current_desired_keys() {
        let mut cache = Cache::default();
        cache.scope(Some((1, 1)));
        let t = texture('b');
        let (tx, _) = pending(&mut cache, t.clone());
        let image = Arc::new(Image {
            width: 64,
            height: 64,
            rgba: vec![7; 64 * 64 * 4],
        });
        tx.send(Ok(Arc::clone(&image))).unwrap();
        let wanted = BTreeSet::from([t.clone()]);
        cache.update((1, 1), Duration::from_secs(1), &wanted);
        assert!(Arc::ptr_eq(
            &cache.images(&wanted)[&t.head_atlas_name()],
            &image
        ));
        assert!(cache.images(&BTreeSet::new()).is_empty());
        cache.scope(None);
        assert!(cache.images(&wanted).is_empty());
        cache.scope(Some((2, 1)));
        assert!(Arc::ptr_eq(
            &cache.images(&wanted)[&t.head_atlas_name()],
            &image
        ));
    }
    #[test]
    fn failures_are_inspectable_and_expire_without_changing_ready_pixels() {
        let mut cache = Cache::default();
        cache.scope(Some((1, 1)));
        let t = texture('c');
        let (tx, _) = pending(&mut cache, t.clone());
        tx.send(Err("controlled failure".into())).unwrap();
        cache.update((1, 1), Duration::from_secs(1), &BTreeSet::from([t.clone()]));
        assert_eq!(cache.failure(&t), Some("controlled failure"));
        cache.update((1, 1), LIFETIME + Duration::from_secs(1), &BTreeSet::new());
        assert!(cache.failure(&t).is_none());
    }
}
