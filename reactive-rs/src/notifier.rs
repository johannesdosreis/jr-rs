use crate::listenable::{Listenable, Listener};
use crate::registry::Registry;

#[derive(Default)]
pub struct Notifier {
    listeners: Registry<Listener>,
}

impl Notifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify_listeners(&self) {
        self.listeners.for_each(|listener| listener());
    }
}

impl Listenable for Notifier {
    fn add_listener(&self, listener: Listener) -> usize {
        self.listeners.insert(listener)
    }

    fn remove_listener(&self, id: usize) {
        self.listeners.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Log, assert_send_sync};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};
    use std::thread;

    fn counter() -> (Arc<AtomicUsize>, Listener) {
        let count = Arc::new(AtomicUsize::new(0));
        let listener_count = count.clone();
        let listener: Listener = Arc::new(move || {
            listener_count.fetch_add(1, Ordering::Relaxed);
        });
        (count, listener)
    }

    #[test]
    fn calls_listeners_in_registration_order() {
        let log = Log::default();
        let n = Notifier::new();
        n.add_listener(log.listener("b"));
        n.add_listener(log.listener("a"));

        n.notify_listeners();

        assert_eq!(log.take(), ["b", "a"]);
    }

    #[test]
    fn calls_same_listener_once_per_registration() {
        let log = Log::default();
        let n = Notifier::new();
        let a = log.listener("a");
        n.add_listener(a.clone());
        n.add_listener(a);

        n.notify_listeners();

        assert_eq!(log.take(), ["a", "a"]);
    }

    #[test]
    fn does_not_call_removed_listener() {
        let log = Log::default();
        let n = Notifier::new();
        let id = n.add_listener(log.listener("a"));
        n.add_listener(log.listener("b"));

        n.remove_listener(id);
        n.notify_listeners();

        assert_eq!(log.take(), ["b"]);
    }

    #[test]
    fn subscription_removes_listener_when_dropped() {
        let log = Log::default();
        let n = Notifier::new();
        let subscription = n.subscribe(log.listener("a"));
        n.notify_listeners();
        assert_eq!(log.take(), ["a"]);

        drop(subscription);
        n.notify_listeners();

        assert!(log.take().is_empty());
    }

    #[test]
    fn listener_removed_during_notification_is_not_called() {
        let log = Log::default();
        let n = Arc::new(Notifier::new());
        let b_id = Arc::new(OnceLock::new());
        let remover = {
            let (log, n, b_id) = (log.clone(), Arc::downgrade(&n), b_id.clone());
            Arc::new(move || {
                log.push("remover");
                if let Some(n) = n.upgrade() {
                    n.remove_listener(*b_id.get().unwrap());
                }
            })
        };
        n.add_listener(remover);
        b_id.set(n.add_listener(log.listener("b"))).unwrap();

        n.notify_listeners();

        assert_eq!(log.take(), ["remover"]);
    }

    #[test]
    fn is_send_and_sync() {
        assert_send_sync::<Notifier>();
    }

    #[test]
    fn can_be_notified_from_many_threads() {
        let n = Notifier::new();
        let (count, listener) = counter();
        n.add_listener(listener);

        thread::scope(|s| {
            for _ in 0..8 {
                s.spawn(|| (0..1000).for_each(|_| n.notify_listeners()));
            }
        });

        assert_eq!(count.load(Ordering::Relaxed), 8000);
    }

    #[test]
    fn listeners_can_be_added_and_removed_while_other_threads_notify() {
        let n = Notifier::new();
        let (count, listener) = counter();
        n.add_listener(listener);

        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| (0..1000).for_each(|_| n.notify_listeners()));
                s.spawn(|| {
                    for _ in 0..1000 {
                        let id = n.add_listener(Arc::new(|| {}));
                        n.remove_listener(id);
                    }
                });
            }
        });

        assert_eq!(count.load(Ordering::Relaxed), 4000);
    }
}
