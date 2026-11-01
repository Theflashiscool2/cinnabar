//! Public profile resolution via the pinned 26.3 authlib service-discovery contract.
use super::{Downloader, Texture, pool};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
// MinecraftServicesEnvironment.PROD, verified in the pinned authlib archive.
const DISCOVERY: &str = "https://discovery.minecraftservices.com/minecraft/client";
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Query {
    Name(String),
    Id([u8; 16]),
}
#[derive(Clone, Debug)]
pub struct Resolved {
    pub id: [u8; 16],
    pub texture: Option<Texture>,
}
fn id(value: &Value) -> Result<[u8; 16], String> {
    let text = value
        .as_str()
        .ok_or("Java public profile UUID missing")?
        .replace('-', "");
    if text.len() != 32 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid Java public profile UUID".into());
    }
    let mut id = [0; 16];
    for (i, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16)
            .map_err(|_| "invalid Java profile UUID")?;
    }
    Ok(id)
}
fn endpoint(discovery: &Value, service: &str, name: &str) -> Result<String, String> {
    discovery
        .get("discovery")
        .and_then(|v| v.get(service))
        .and_then(|v| v.get("endpoints"))
        .and_then(|v| v.get(name))
        .and_then(|v| v.get("uri"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "Java public profile discovery endpoint missing".into())
}
fn profile(data: &Value) -> Result<Resolved, String> {
    let id = id(&data["id"])?;
    let mut texture = None;
    if let Some(properties) = data.get("properties") {
        for p in properties
            .as_array()
            .ok_or("Java public profile property list expected")?
        {
            if p.get("name").and_then(Value::as_str) == Some("textures") {
                texture = Texture::from_property(
                    p.get("value")
                        .and_then(Value::as_str)
                        .ok_or("Java public profile texture missing")?,
                )?;
                break;
            }
        }
    }
    Ok(Resolved { id, texture })
}
impl Downloader {
    async fn public_json(&self, url: &str, cancel: &AtomicBool) -> Result<Value, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Java public profile cancelled".into());
        }
        let url = reqwest::Url::parse(url).map_err(|_| "Java public profile URL invalid")?;
        // Discovery is trusted only over TLS; never forward cookies/account credentials or
        // permit discovery data to access local services. Domain changes remain explicit.
        let host = url.host_str().ok_or("Java public profile host missing")?;
        if url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || !(host.ends_with(".minecraftservices.com") || host.ends_with(".mojang.com"))
        {
            return Err("unsupported Java public profile service host".into());
        }
        let mut response = self
            .0
            .get(url)
            .send()
            .await
            .map_err(|_| "Java public profile network request failed")?;
        if !response.status().is_success() {
            return Err(format!(
                "Java public profile HTTP {}",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Java public profile response failed")?
        {
            if cancel.load(Ordering::Relaxed) {
                return Err("Java public profile cancelled".into());
            }
            if bytes.len().saturating_add(chunk.len()) > super::PROPERTY_BUDGET {
                return Err("Java public profile exceeds processing budget".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| "invalid Java public profile JSON".into())
    }
    pub(super) async fn profile(
        &self,
        query: &Query,
        cancel: &AtomicBool,
    ) -> Result<Resolved, String> {
        let discovery = self.public_json(DISCOVERY, cancel).await?;
        let uuid = match query {
            Query::Id(id) => *id,
            Query::Name(name) => {
                let url = endpoint(&discovery, "profiles", "getByName")?;
                // Encode as one URL path component, never interpolate server text as a URL.
                let mut encoded = reqwest::Url::parse("https://invalid.invalid/").unwrap();
                encoded
                    .path_segments_mut()
                    .unwrap()
                    .push(&name.to_lowercase());
                let name = encoded.path().trim_start_matches('/');
                id(&self
                    .public_json(&url.replace("{name}", name), cancel)
                    .await?["id"])?
            }
        };
        let uuid = uuid.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let mut url = reqwest::Url::parse(
            &endpoint(&discovery, "session", "getProfileById")?.replace("{profileId}", &uuid),
        )
        .map_err(|_| "Java public profile endpoint invalid")?;
        url.query_pairs_mut().append_pair("unsigned", "true");
        profile(&self.public_json(url.as_str(), cancel).await?)
    }
}
struct Entry {
    touched: Duration,
    state: State,
}
enum State {
    Pending(Reply, Arc<AtomicBool>),
    Ready(Resolved),
    Failed(String),
}
impl Drop for State {
    fn drop(&mut self) {
        if let Self::Pending(_, cancel) = self {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}
pub type Reply = mpsc::Receiver<Result<Resolved, String>>;
#[derive(Default)]
pub struct Cache {
    scope: Option<(u64, u64)>,
    entries: BTreeMap<Query, Entry>,
}
impl Cache {
    pub fn scope(&mut self, scope: Option<(u64, u64)>) {
        if self.scope != scope {
            self.entries
                .retain(|_, e| !matches!(e.state, State::Pending(..)));
            self.scope = scope;
        }
    }
    pub fn update(&mut self, scope: (u64, u64), now: Duration, desired: &BTreeSet<Query>) {
        self.scope(Some(scope));
        self.entries.retain(|q, e| {
            now.saturating_sub(e.touched) < super::cache::LIFETIME
                && (!matches!(e.state, State::Pending(..)) || desired.contains(q))
        });
        for query in desired {
            if let Some(e) = self.entries.get_mut(query) {
                e.touched = now;
                if let State::Pending(reply, _) = &e.state {
                    match reply.try_recv() {
                        Ok(Ok(r)) => e.state = State::Ready(r),
                        Ok(Err(e2)) => e.state = State::Failed(e2),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            e.state = State::Failed("Java public profile worker stopped".into())
                        }
                        Err(mpsc::TryRecvError::Empty) => {}
                    }
                }
                continue;
            }
            if self.entries.len() >= super::cache::CAPACITY {
                let evict = self
                    .entries
                    .iter()
                    .filter(|(q, _)| !desired.contains(*q))
                    .min_by_key(|(_, e)| e.touched)
                    .map(|(q, _)| q.clone());
                if let Some(key) = evict {
                    self.entries.remove(&key);
                } else {
                    break;
                }
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let state = match pool::request_profile(query.clone(), Arc::clone(&cancel)) {
                Ok(reply) => State::Pending(reply, cancel),
                Err(pool::Error::Busy | pool::Error::Cancelled) => continue,
                Err(pool::Error::Unavailable(e)) => State::Failed(e),
            };
            self.entries.insert(
                query.clone(),
                Entry {
                    touched: now,
                    state,
                },
            );
        }
    }
    pub fn get(&self, query: &Query) -> Option<&Resolved> {
        if self.scope.is_none() {
            return None;
        }
        match &self.entries.get(query)?.state {
            State::Ready(r) => Some(r),
            _ => None,
        }
    }
    pub fn failure(&self, query: &Query) -> Option<&str> {
        if !self.entries.contains_key(query) && self.entries.len() >= super::cache::CAPACITY {
            return Some("Java public profile working-set budget exhausted");
        }
        match &self.entries.get(query)?.state {
            State::Failed(e) => Some(e),
            _ => None,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_profile_schema_and_missing_texture_are_distinct_from_invalid_data() {
        let p=profile(&serde_json::json!({"id":"12345678123456789abcdef012345678","name":"Fixture","properties":[]})).unwrap();
        assert_eq!(
            p.id,
            [
                0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34,
                0x56, 0x78
            ]
        );
        assert!(p.texture.is_none());
        assert!(profile(&serde_json::json!({"id":"/path"})).is_err());
        assert!(
            profile(&serde_json::json!({"id":"12345678123456789abcdef012345678","properties":{}}))
                .is_err()
        );
    }
    #[test]
    fn discovery_requires_named_native_endpoint() {
        let d = serde_json::json!({"discovery":{"session":{"endpoints":{"getProfileById":{"uri":"https://example.invalid/{profileId}"}}}}});
        assert_eq!(
            endpoint(&d, "session", "getProfileById").unwrap(),
            "https://example.invalid/{profileId}"
        );
        assert!(endpoint(&d, "session", "getByName").is_err());
    }
    #[test]
    fn scope_retirement_cancels_profile_lookup_and_hides_old_results() {
        let mut cache = Cache::default();
        cache.scope(Some((1, 1)));
        let q = Query::Id([0; 16]);
        let (tx, rx) = mpsc::sync_channel(1);
        let cancel = Arc::new(AtomicBool::new(false));
        cache.entries.insert(
            q.clone(),
            Entry {
                touched: Duration::ZERO,
                state: State::Pending(rx, Arc::clone(&cancel)),
            },
        );
        cache.scope(Some((2, 1)));
        assert!(cancel.load(Ordering::Relaxed));
        assert!(tx.send(Err("obsolete".into())).is_err());
        assert!(cache.get(&q).is_none());
        cache.entries.insert(
            q.clone(),
            Entry {
                touched: Duration::ZERO,
                state: State::Ready(Resolved {
                    id: [0; 16],
                    texture: None,
                }),
            },
        );
        assert!(cache.get(&q).is_some());
        cache.scope(None);
        assert!(cache.get(&q).is_none());
        cache.update((3, 1), super::super::cache::LIFETIME, &BTreeSet::new());
        assert!(cache.entries.is_empty());
    }
}
