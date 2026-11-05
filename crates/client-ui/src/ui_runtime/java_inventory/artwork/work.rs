//! Single replaceable native-artwork work queue and publication fence.
use super::{Catalog, HeadTextures, SessionIcon, SessionIcons, Stack};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex},
};
pub(super) type Ticket = (u64, u64, u64, u64);
pub(super) struct Request {
    pub(super) ticket: Ticket,
    pub(super) stacks: BTreeMap<String, Stack>,
    pub(super) heads: Arc<HeadTextures>,
}
#[derive(Default)]
pub(super) struct Shared {
    pub(super) closed: bool,
    pub(super) installed: Option<Arc<SessionIcons>>,
    pub(super) desired: Option<Ticket>,
    pub(super) request: Option<Request>,
    pub(super) result: Option<(Ticket, Arc<SessionIcons>)>,
}
impl Shared {
    fn finish(&mut self, ticket: Ticket, icons: Arc<SessionIcons>) {
        if !self.closed && self.desired == Some(ticket) {
            self.result = Some((ticket, icons));
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
        for (key, stack) in request.stacks {
            {
                let work = shared.0.lock().unwrap();
                if work.closed {
                    return;
                }
                if work.desired != Some(request.ticket) {
                    break;
                }
            }
            match catalog.icon_with_heads(&stack, &request.heads) {
                Ok(image) => icons.icons.push(SessionIcon {
                    identifier: Arc::from(key),
                    metadata: 0,
                    width: image.width,
                    height: image.height,
                    rgba8: image.rgba.into_boxed_slice(),
                }),
                Err(error) => {
                    icons.misses.insert(
                        Arc::from(key),
                        format!("{}: {error}", stack.item).into_boxed_str(),
                    );
                }
            }
        }
        let mut work = shared.0.lock().unwrap();
        if work.closed {
            return;
        }
        work.finish(request.ticket, Arc::new(icons));
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
        let icons = Arc::new(SessionIcons::default());
        work.desired = Some(replacement);
        work.finish(first, icons.clone());
        assert!(work.result.is_none());
        work.desired = Some(reloaded);
        work.finish(replacement, icons.clone());
        assert!(work.result.is_none());
        work.finish(reloaded, icons.clone());
        assert_eq!(work.result.take().unwrap().0, reloaded);
        work.closed = true;
        work.finish(reloaded, icons);
        assert!(work.result.is_none());
    }
}
