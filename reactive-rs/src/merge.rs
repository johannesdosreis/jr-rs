use crate::listenable::{Listenable, Listener};
use crate::registry::Registry;

pub struct MergeListenable {
    children: Vec<Box<dyn Listenable>>,
    child_ids: Registry<Vec<usize>>,
}

impl MergeListenable {
    pub fn new(children: Vec<Box<dyn Listenable>>) -> Self {
        MergeListenable {
            children,
            child_ids: Registry::default(),
        }
    }

    fn remove_from_children(&self, child_ids: Vec<usize>) {
        for (child, id) in self.children.iter().zip(child_ids) {
            child.remove_listener(id);
        }
    }
}

impl Listenable for MergeListenable {
    fn add_listener(&self, listener: Listener) -> usize {
        let child_ids = self
            .children
            .iter()
            .map(|child| child.add_listener(listener.clone()))
            .collect();
        self.child_ids.insert(child_ids)
    }

    fn remove_listener(&self, id: usize) {
        if let Some(child_ids) = self.child_ids.remove(id) {
            self.remove_from_children(child_ids);
        }
    }
}

impl Drop for MergeListenable {
    fn drop(&mut self) {
        for child_ids in self.child_ids.drain() {
            self.remove_from_children(child_ids);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Notifier;
    use crate::test_support::{Log, assert_send_sync};
    use std::sync::Arc;

    fn children() -> (Arc<Notifier>, Arc<Notifier>, MergeListenable) {
        let x = Arc::new(Notifier::new());
        let y = Arc::new(Notifier::new());
        let merge = MergeListenable::new(vec![Box::new(x.clone()), Box::new(y.clone())]);
        (x, y, merge)
    }

    #[test]
    fn calls_listener_when_any_child_notifies() {
        let log = Log::default();
        let (x, y, merge) = children();
        merge.add_listener(log.listener("a"));

        x.notify_listeners();
        y.notify_listeners();

        assert_eq!(log.take(), ["a", "a"]);
    }

    #[test]
    fn remove_listener_removes_it_from_all_children() {
        let log = Log::default();
        let (x, y, merge) = children();
        let id = merge.add_listener(log.listener("a"));

        merge.remove_listener(id);
        x.notify_listeners();
        y.notify_listeners();

        assert!(log.take().is_empty());
    }

    #[test]
    fn remove_listener_keeps_other_child_listeners() {
        let log = Log::default();
        let (x, _y, merge) = children();
        x.add_listener(log.listener("b"));
        let id = merge.add_listener(log.listener("a"));

        merge.remove_listener(id);
        x.notify_listeners();

        assert_eq!(log.take(), ["b"]);
    }

    #[test]
    fn subscription_removes_listener_when_dropped() {
        let log = Log::default();
        let (x, _y, merge) = children();
        let subscription = merge.subscribe(log.listener("a"));
        x.notify_listeners();
        assert_eq!(log.take(), ["a"]);

        drop(subscription);
        x.notify_listeners();

        assert!(log.take().is_empty());
    }

    #[test]
    fn dropping_merge_removes_its_listeners_from_children() {
        let log = Log::default();
        let (x, y, merge) = children();
        merge.add_listener(log.listener("a"));

        drop(merge);
        x.notify_listeners();
        y.notify_listeners();

        assert!(log.take().is_empty());
    }

    #[test]
    fn is_send_and_sync() {
        assert_send_sync::<MergeListenable>();
    }
}
