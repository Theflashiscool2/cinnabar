//! Exact view-specific held resources, bounded by the current hotbar and offhand.
use super::work::Ticket;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
pub(super) type Key = (String, bool);
/// Selected views come first; only currently reachable hotbar inputs are warmed afterward.
pub(super) fn priority(
    hands: &[Option<String>; 2],
    hotbar: impl IntoIterator<Item = String>,
) -> Vec<Key> {
    let mut keys: Vec<_> = hands
        .iter()
        .enumerate()
        .filter_map(|(hand, key)| Some((key.as_ref()?.clone(), hand == 1)))
        .collect();
    for key in hotbar {
        let entry = (key, false);
        if !keys.contains(&entry) {
            keys.push(entry);
        }
    }
    keys
}
pub(super) struct Resources<T> {
    ticket: Option<Ticket>,
    desired: BTreeSet<Key>,
    ready: BTreeMap<Key, Result<Option<Arc<T>>, String>>,
}
impl<T> Default for Resources<T> {
    fn default() -> Self {
        Self {
            ticket: None,
            desired: BTreeSet::new(),
            ready: BTreeMap::new(),
        }
    }
}
impl<T> Resources<T> {
    /// Retains only exact inputs in the same world, catalog owner and head-texture generation.
    pub(super) fn begin(&mut self, ticket: Ticket, keys: impl IntoIterator<Item = Key>) {
        if !self.ticket.is_some_and(|old| {
            old.0 == ticket.0 && old.1 == ticket.1 && old.2 <= ticket.2 && old.3 == ticket.3
        }) {
            self.ready.clear();
        }
        self.ticket = Some(ticket);
        self.desired = keys.into_iter().collect();
        self.ready.retain(|key, _| self.desired.contains(key));
    }
    /// Commits independently of GUI icon work; late requests cannot populate a new scope.
    pub(super) fn publish(
        &mut self,
        ticket: Ticket,
        key: Key,
        result: Result<Option<Arc<T>>, String>,
    ) {
        if self.ticket == Some(ticket) && self.desired.contains(&key) {
            self.ready.insert(key, result);
        }
    }
    pub(super) fn get(&self, key: &Key) -> Option<&Result<Option<Arc<T>>, String>> {
        self.ready.get(key)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(item: &str, offhand: bool) -> Key {
        (item.into(), offhand)
    }
    #[test]
    fn selected_resource_publishes_before_other_hotbar_and_icon_work() {
        let ticket = (7, 2, 19, 1);
        let selected = key("shears-exact", false);
        let next = key("wool-exact", false);
        let mut resources = Resources::default();
        let order = priority(
            &[Some(selected.0.clone()), Some("shield".into())],
            [next.0.clone(), selected.0.clone()],
        );
        assert_eq!(order, [selected.clone(), key("shield", true), next.clone()]);
        resources.begin(ticket, order);
        resources.publish(ticket, selected.clone(), Ok(Some(Arc::new(41))));
        assert_eq!(
            **resources
                .get(&selected)
                .unwrap()
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap(),
            41
        );
        assert!(resources.get(&next).is_none());
        resources.publish(ticket, next.clone(), Ok(Some(Arc::new(73))));
        // A selection-only revision can immediately use the exact prewarmed model.
        resources.begin((7, 2, 20, 1), [next.clone(), selected]);
        assert_eq!(
            **resources
                .get(&next)
                .unwrap()
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap(),
            73
        );
        assert!(resources.get(&key("wool-different-count", false)).is_none());
        assert!(resources.get(&key("wool-exact", true)).is_none());
    }
    #[test]
    fn resources_retire_on_exact_input_scope_and_texture_changes() {
        let original = (7, 2, 19, 1);
        let held = key("held", false);
        for replacement in [(8, 2, 20, 1), (7, 3, 20, 1), (7, 2, 20, 2), (7, 2, 18, 1)] {
            let mut resources = Resources::default();
            resources.begin(original, [held.clone()]);
            resources.publish(original, held.clone(), Ok(Some(Arc::new(41))));
            resources.begin(replacement, [held.clone()]);
            assert!(resources.get(&held).is_none());
            resources.publish(original, held.clone(), Ok(Some(Arc::new(97))));
            assert!(resources.get(&held).is_none());
        }
        let mut resources = Resources::default();
        resources.begin(original, [held.clone()]);
        let retired = Arc::new(41);
        resources.publish(original, held.clone(), Ok(Some(retired.clone())));
        resources.begin((7, 2, 20, 1), []);
        assert!(resources.get(&held).is_none());
        assert_eq!(Arc::strong_count(&retired), 1);
        // Catalog replacement constructs a fresh artwork owner, not a reusable global cache.
        let reloaded = Resources::<i32>::default();
        assert!(reloaded.get(&held).is_none());
    }
    #[test]
    fn empty_and_unsupported_geometry_remain_real_results_without_fallback() {
        let ticket = (7, 2, 19, 1);
        let empty = key("empty-model", false);
        let unsupported = key("unsupported-model", false);
        let mut resources = Resources::<u32>::default();
        resources.begin(ticket, [empty.clone(), unsupported.clone()]);
        resources.publish(ticket, empty.clone(), Ok(None));
        resources.publish(
            ticket,
            unsupported.clone(),
            Err("unsupported native model".into()),
        );
        assert!(matches!(resources.get(&empty), Some(Ok(None))));
        assert!(
            matches!(resources.get(&unsupported), Some(Err(e)) if e == "unsupported native model")
        );
        resources.begin((7, 2, 20, 1), [empty.clone(), unsupported.clone()]);
        assert!(matches!(resources.get(&empty), Some(Ok(None))));
        assert!(matches!(resources.get(&unsupported), Some(Err(_))));
    }
}
