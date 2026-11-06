//! Single replaceable native-artwork work queue and publication fence.
use super::{Catalog, HeadTextures, SessionIcon, SessionIcons, Stack};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
};
pub(super) type Held = Option<(String, Arc<render_model::equipment::java_item::Prepared>)>;
#[derive(Default)]
pub(super) struct Output {
    pub(super) icons: Arc<SessionIcons>,
    pub(super) hands: [Held; 2],
    pub(super) held_misses: Vec<String>,
    pub(super) sources: [Option<super::continuity::Source>; 2],
}
pub(super) type Ticket = (u64, u64, u64, u64);
pub(super) struct Request {
    pub(super) ticket: Ticket,
    pub(super) stacks: BTreeMap<String, Stack>,
    pub(super) heads: Arc<HeadTextures>,
    pub(super) hands: [Option<String>; 2],
}
#[derive(Default)]
pub(super) struct Shared {
    pub(super) closed: bool,
    pub(super) installed: Option<Arc<SessionIcons>>,
    pub(super) installed_ticket: Option<Ticket>,
    pub(super) installed_sources: [Option<super::continuity::Source>; 2],
    pub(super) desired_sources: [Option<super::continuity::Source>; 2],
    pub(super) inventory_sources: BTreeMap<(i32, u64, usize), super::continuity::Source>,
    pub(super) publications: Vec<super::continuity::Publication>,
    pub(super) desired: Option<Ticket>,
    pub(super) request: Option<Request>,
    pub(super) result: Option<(Ticket, Output)>,
    pub(super) hands: [Held; 2],
    pub(super) held_misses: Vec<String>,
}
impl Shared {
    fn finish(&mut self, ticket: Ticket, output: Output) {
        if !self.closed && self.desired == Some(ticket) {
            self.result = Some((ticket, output));
        }
    }
}
pub(super) fn worker(catalog: Arc<Catalog>, shared: Arc<(Mutex<Shared>, Condvar)>) {
    loop {
        let request = {
            let mut work = shared.0.lock().unwrap();
            while !work.closed && work.request.is_none() {
                work = shared.1.wait(work).unwrap();
            }
            if work.closed {
                return;
            }
            work.request.take().unwrap()
        };
        let mut icons = SessionIcons::default();
        for (key, stack) in &request.stacks {
            {
                let work = shared.0.lock().unwrap();
                if work.closed {
                    return;
                }
                if work.desired != Some(request.ticket) {
                    break;
                }
            }
            match catalog.icon_with_heads(stack, &request.heads) {
                Ok(image) => icons.icons.push(SessionIcon {
                    identifier: Arc::from(key.as_str()),
                    metadata: 0,
                    width: image.width,
                    height: image.height,
                    rgba8: image.rgba.into_boxed_slice(),
                }),
                Err(error) => {
                    icons.misses.insert(
                        Arc::from(key.as_str()),
                        format!("{}: {error}", stack.item).into_boxed_str(),
                    );
                }
            }
        }
        let sources = std::array::from_fn(|i| {
            let key = request.hands[i].as_ref()?;
            Some(super::continuity::Source {
                key: key.clone(),
                item: request.stacks.get(key)?.item.clone(),
            })
        });
        let mut hands = std::array::from_fn(|_| None);
        let mut held_misses = Vec::new();
        for (hand, key) in request.hands.into_iter().enumerate() {
            if let Some(key) = key
                && let Some(stack) = request.stacks.get(&key)
            {
                let view = if hand == 0 {
                    assets::registry::java::items::View::FirstPersonRight
                } else {
                    assets::registry::java::items::View::FirstPersonLeft
                };
                let result = catalog
                    .geometry_in(stack, &request.heads, view)
                    .map_err(|e| e.to_string())
                    .and_then(|quads| {
                        render_model::equipment::java_item::prepare(quads, hand == 1)
                    });
                match result {
                    Ok(Some(prepared)) => hands[hand] = Some((key, Arc::new(prepared))),
                    Ok(None) => {}
                    Err(error) => held_misses.push(format!("{}: {error}", stack.item)),
                }
            }
        }
        let mut work = shared.0.lock().unwrap();
        if work.closed {
            return;
        }
        work.finish(
            request.ticket,
            Output {
                icons: Arc::new(icons),
                hands,
                held_misses,
                sources,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_artwork_cannot_replace_new_menu_resource_or_retired_session() {
        let first = (7, 2, 19, 1);
        let replacement = (7, 2, 20, 1);
        let reloaded = (7, 2, 20, 2);
        let mut work = Shared::default();
        let output = || Output {
            icons: Arc::new(SessionIcons::default()),
            ..Default::default()
        };
        work.desired = Some(replacement);
        work.finish(first, output());
        assert!(work.result.is_none());
        work.desired = Some(reloaded);
        work.finish(replacement, output());
        assert!(work.result.is_none());
        work.finish(reloaded, output());
        assert_eq!(work.result.take().unwrap().0, reloaded);
        work.closed = true;
        work.finish(reloaded, output());
        assert!(work.result.is_none());
    }
}
