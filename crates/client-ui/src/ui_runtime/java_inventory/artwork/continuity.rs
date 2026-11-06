//! Retain committed graphics during same-item preparation, never transaction authority.
use super::{
    SessionIcons,
    work::{Shared, Ticket},
};
use std::sync::{Arc, Weak};
#[derive(Clone, Debug)]
pub(super) struct Source {
    pub key: String,
    pub item: String,
}
pub(super) struct Publication {
    ticket: Ticket,
    icons: Weak<SessionIcons>,
    sources: [Option<Source>; 2],
}
fn compatible(installed: Option<Ticket>, desired: Option<Ticket>) -> bool {
    installed
        .zip(desired)
        .is_some_and(|(a, b)| a.0 == b.0 && a.1 == b.1 && a.2 <= b.2 && a.3 == b.3)
}
impl Shared {
    pub(super) fn begin(&mut self, ticket: Ticket, sources: [Option<Source>; 2]) {
        self.desired = Some(ticket);
        self.desired_sources = sources;
    }
    pub(super) fn can_hold(&self, hand: usize, key: &str) -> bool {
        !self.closed
            && compatible(self.installed_ticket, self.desired)
            && self
                .installed_sources
                .get(hand)
                .zip(self.desired_sources.get(hand))
                .is_some_and(|(old, new)| {
                    old.as_ref()
                        .zip(new.as_ref())
                        .is_some_and(|(old, new)| new.key == key && old.item == new.item)
                })
    }
    pub(super) fn remember(
        &mut self,
        ticket: Ticket,
        icons: &Arc<SessionIcons>,
        sources: [Option<Source>; 2],
    ) {
        // Do not keep textures alive. Normally only the installed UI page, pending packing
        // and newest worker output retain these Arcs. Preserve oldest live page + latest stages.
        self.publications.retain(|p| p.icons.strong_count() > 0);
        if self.publications.len() >= 8 {
            self.publications.remove(1);
        }
        self.publications.push(Publication {
            ticket,
            icons: Arc::downgrade(icons),
            sources,
        });
    }
    pub(super) fn aliases_for(
        &self,
        uploaded: &Arc<SessionIcons>,
    ) -> [Option<(String, String)>; 2] {
        if self.closed {
            return Default::default();
        }
        let Some(page) = self
            .publications
            .iter()
            .find(|p| p.icons.ptr_eq(&Arc::downgrade(uploaded)))
        else {
            return Default::default();
        };
        if !compatible(Some(page.ticket), self.desired) {
            return Default::default();
        }
        std::array::from_fn(|i| {
            let old = page.sources[i].as_ref()?;
            let new = self.desired_sources[i].as_ref()?;
            if old.item != new.item || old.key == new.key {
                return None;
            }
            if self.installed_ticket == self.desired
                && self
                    .installed
                    .as_ref()
                    .is_some_and(|icons| icons.misses.contains_key(new.key.as_str()))
            {
                return None;
            }
            Some((new.key.clone(), old.key.clone()))
        })
    }
}
/// Prefer the exact current entry; bridge only to the same hand's actually uploaded source.
pub(in crate::ui_runtime::java_inventory) fn resolve<T>(
    key: &str,
    aliases: &[Option<(String, String)>; 2],
    mut lookup: impl FnMut(&str) -> Option<T>,
) -> Option<T> {
    lookup(key).or_else(|| {
        aliases
            .iter()
            .flatten()
            .find(|(new, _)| new == key)
            .and_then(|(_, old)| lookup(old))
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn source(key: &str, item: &str) -> Option<Source> {
        Some(Source {
            key: key.into(),
            item: item.into(),
        })
    }
    #[test]
    fn pending_same_item_has_current_hand_provenance_and_strict_scope() {
        let mut shared = Shared {
            installed_ticket: Some((7, 2, 19, 1)),
            installed_sources: [source("old", "sword"), None],
            ..Default::default()
        };
        shared.begin((7, 2, 20, 1), [source("new", "sword"), None]);
        assert!(shared.can_hold(0, "new"));
        assert!(!shared.can_hold(0, "old"));
        assert!(!shared.can_hold(1, "new"));
        for ticket in [(8, 2, 20, 1), (7, 3, 20, 1), (7, 2, 20, 2), (7, 2, 18, 1)] {
            shared.begin(ticket, [source("new", "sword"), None]);
            assert!(!shared.can_hold(0, "new"));
        }
        shared.begin((7, 2, 20, 1), [source("new", "dirt"), None]);
        assert!(!shared.can_hold(0, "new"));
        shared.begin((7, 2, 20, 1), [None, None]);
        assert!(!shared.can_hold(0, "old"));
        shared.begin((7, 2, 20, 1), [source("new", "sword"), None]);
        shared.closed = true;
        assert!(!shared.can_hold(0, "new"));
    }
    #[test]
    fn actual_uploaded_page_bridges_worker_progress_without_alias_chains_or_leaks() {
        let uploaded = Arc::new(SessionIcons::default());
        let mut shared = Shared::default();
        shared.remember(
            (7, 2, 19, 1),
            &uploaded,
            [source("uploaded", "sword"), None],
        );
        for revision in 20..200 {
            let key = format!("next{revision}");
            let output = Arc::new(SessionIcons::default());
            shared.begin((7, 2, revision, 1), [source(&key, "sword"), None]);
            shared.remember((7, 2, revision, 1), &output, shared.desired_sources.clone());
            shared.installed = Some(output);
            shared.installed_ticket = shared.desired;
            assert_eq!(
                shared.aliases_for(&uploaded),
                [Some((key, "uploaded".into())), None]
            );
            assert!(shared.publications.len() <= 3);
        }
        let latest = shared.installed.clone().unwrap();
        shared.begin((7, 2, 200, 1), [source("next200", "sword"), None]);
        assert_eq!(
            shared.aliases_for(&latest),
            [Some(("next200".into(), "next199".into())), None]
        );
        shared.begin((7, 2, 201, 1), [source("new", "dirt"), None]);
        assert_eq!(shared.aliases_for(&uploaded), [None, None]);
        shared.begin((7, 3, 201, 1), [source("new", "sword"), None]);
        assert_eq!(shared.aliases_for(&uploaded), [None, None]);
    }
    #[test]
    fn existing_atlas_pixels_bridge_upload_then_exact_current_entry_takes_over() {
        let aliases = [
            Some(("new".into(), "old".into())),
            Some(("off-new".into(), "off-old".into())),
        ];
        let mut page = std::collections::BTreeMap::from([("old", 41), ("off-old", 73)]);
        assert_eq!(resolve("new", &aliases, |k| page.get(k).copied()), Some(41));
        assert_eq!(
            resolve("off-new", &aliases, |k| page.get(k).copied()),
            Some(73)
        );
        assert_eq!(
            resolve("unrelated", &aliases, |k| page.get(k).copied()),
            None
        );
        page.insert("new", 97);
        assert_eq!(resolve("new", &aliases, |k| page.get(k).copied()), Some(97));
    }
    #[test]
    fn failed_output_and_resource_replacement_do_not_mask_missing_artwork() {
        let uploaded = Arc::new(SessionIcons::default());
        let mut shared = Shared::default();
        shared.remember((7, 2, 19, 1), &uploaded, [source("old", "sword"), None]);
        shared.begin((7, 2, 20, 1), [source("new", "sword"), None]);
        let mut failed = SessionIcons::default();
        failed
            .misses
            .insert("new".into(), "unsupported current model".into());
        shared.installed = Some(Arc::new(failed));
        shared.installed_ticket = shared.desired;
        assert_eq!(shared.aliases_for(&uploaded), [None, None]);
        shared.begin((7, 2, 21, 2), [source("new", "sword"), None]);
        assert_eq!(shared.aliases_for(&uploaded), [None, None]);
        shared.closed = true;
        assert_eq!(shared.aliases_for(&uploaded), [None, None]);
    }
}
