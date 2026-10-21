use std::sync::{Mutex, MutexGuard};

use crate::registry::Registry;
use crate::sync::lock;
use crate::value_listenable::{ValueListenable, ValueListener};

pub struct ValueNotifier<T> {
    listeners: Registry<ValueListener<T>>,
    value: Mutex<T>,
}

impl<T> ValueNotifier<T> {
    pub fn new(value: T) -> Self {
        ValueNotifier {
            listeners: Registry::default(),
            value: Mutex::new(value),
        }
    }
}

impl<T: Clone> ValueNotifier<T> {
    pub fn get(&self) -> T {
        lock(&self.value).clone()
    }
}

impl<T: Clone + PartialEq> ValueNotifier<T> {
    /// Notifies the listeners only when the value actually changes.
    /// The lock is released before the listeners run, so notifications from
    /// concurrent `set`s may interleave; call `get` for the latest value.
    pub fn set(&self, value: T) {
        self.store(lock(&self.value), value);
    }

    /// Changes the value in place, e.g. `n.update(|point| point.x = 10)`, and
    /// notifies like `set`. The value stays locked while `modify` runs, so
    /// concurrent updates never lose writes, but `modify` must not call back
    /// into this notifier or it deadlocks. `modify` works on a copy: if it
    /// panics, the value is left untouched.
    pub fn update(&self, modify: impl FnOnce(&mut T)) {
        let current = lock(&self.value);
        let mut value = current.clone();
        modify(&mut value);
        self.store(current, value);
    }

    fn store(&self, mut current: MutexGuard<'_, T>, value: T) {
        if *current == value {
            return;
        }
        *current = value.clone();
        drop(current);
        self.listeners.for_each(|listener| listener(value.clone()));
    }
}

impl<T: Send> ValueListenable<T> for ValueNotifier<T> {
    fn add_listener(&self, listener: ValueListener<T>) -> usize {
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
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    #[derive(Clone, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[test]
    fn get_returns_initial_value() {
        let n = ValueNotifier::new(1);

        assert_eq!(n.get(), 1);
    }

    #[test]
    fn set_updates_value_and_notifies_listeners() {
        let log = Log::default();
        let n = ValueNotifier::new(1);
        n.add_listener(log.value_listener());

        n.set(2);

        assert_eq!(n.get(), 2);
        assert_eq!(log.take(), ["value 2"]);
    }

    #[test]
    fn set_with_same_value_does_not_notify() {
        let log = Log::default();
        let n = ValueNotifier::new(1);
        n.add_listener(log.value_listener());

        n.set(1);

        assert!(log.take().is_empty());
    }

    #[test]
    fn does_not_call_removed_listener() {
        let log = Log::default();
        let n = ValueNotifier::new(1);
        let id = n.add_listener(log.value_listener());

        n.remove_listener(id);
        n.set(2);

        assert!(log.take().is_empty());
    }

    #[test]
    fn subscription_removes_listener_when_dropped() {
        let log = Log::default();
        let n = ValueNotifier::new(1);
        let subscription = n.subscribe(log.value_listener());
        n.set(2);
        assert_eq!(log.take(), ["value 2"]);

        drop(subscription);
        n.set(3);

        assert!(log.take().is_empty());
    }

    #[test]
    fn listener_can_read_value_during_notification() {
        let log = Log::default();
        let n = Arc::new(ValueNotifier::new(0));
        let reader = {
            let (log, n) = (log.clone(), Arc::downgrade(&n));
            Arc::new(move |v| {
                let current = n.upgrade().unwrap().get();
                log.push(format!("got {v}, current {current}"));
            })
        };
        n.add_listener(reader);

        n.set(5);

        assert_eq!(log.take(), ["got 5, current 5"]);
    }

    #[test]
    fn update_changes_one_field_and_notifies_listeners() {
        let log = Log::default();
        let n = ValueNotifier::new(Point { x: 1, y: 2 });
        n.add_listener(log.value_listener());

        n.update(|point| point.x = 10);

        assert_eq!(n.get(), Point { x: 10, y: 2 });
        assert_eq!(log.take(), ["value Point { x: 10, y: 2 }"]);
    }

    #[test]
    fn update_without_change_does_not_notify() {
        let log = Log::default();
        let n = ValueNotifier::new(Point { x: 1, y: 2 });
        n.add_listener(log.value_listener());

        n.update(|point| point.x = 1);

        assert!(log.take().is_empty());
    }

    #[test]
    fn update_keeps_value_when_modify_panics() {
        let log = Log::default();
        let n = ValueNotifier::new(Point { x: 1, y: 2 });
        n.add_listener(log.value_listener());

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            n.update(|point| {
                point.x = 10;
                panic!("modify failed");
            })
        }));

        assert!(result.is_err());
        assert_eq!(n.get(), Point { x: 1, y: 2 });
        assert!(log.take().is_empty());
    }

    #[test]
    fn is_send_and_sync() {
        assert_send_sync::<ValueNotifier<i32>>();
    }

    #[test]
    fn can_be_set_from_many_threads() {
        let n = ValueNotifier::new(-1);
        let count = Arc::new(AtomicUsize::new(0));
        let listener_count = count.clone();
        n.add_listener(Arc::new(move |_| {
            listener_count.fetch_add(1, Ordering::Relaxed);
        }));

        thread::scope(|s| {
            for t in 0..8 {
                let n = &n;
                s.spawn(move || (0..1000).for_each(|i| n.set(t * 1000 + i)));
            }
        });

        assert_eq!(count.load(Ordering::Relaxed), 8000);
        assert!((0..8000).contains(&n.get()));
    }

    #[test]
    fn concurrent_updates_of_different_fields_do_not_lose_writes() {
        let n = ValueNotifier::new(Point { x: 0, y: 0 });

        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| (0..1000).for_each(|_| n.update(|point| point.x += 1)));
                s.spawn(|| (0..1000).for_each(|_| n.update(|point| point.y += 1)));
            }
        });

        assert_eq!(n.get(), Point { x: 4000, y: 4000 });
    }
}
