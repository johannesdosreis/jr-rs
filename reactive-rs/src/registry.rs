use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::sync::lock;

/// Values keyed by increasing ids, so iteration follows insertion order.
pub(crate) struct Registry<V> {
    entries: Mutex<BTreeMap<usize, V>>,
    next_id: AtomicUsize,
}

impl<V> Default for Registry<V> {
    fn default() -> Self {
        Registry {
            entries: Mutex::new(BTreeMap::new()),
            next_id: AtomicUsize::new(0),
        }
    }
}

impl<V> Registry<V> {
    pub(crate) fn insert(&self, value: V) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.entries).insert(id, value);
        id
    }

    pub(crate) fn remove(&self, id: usize) -> Option<V> {
        lock(&self.entries).remove(&id)
    }

    pub(crate) fn drain(&self) -> Vec<V> {
        std::mem::take(&mut *lock(&self.entries))
            .into_values()
            .collect()
    }

    /// No lock is held while `call` runs, so it may add or remove entries.
    /// Entries removed meanwhile are skipped; entries added meanwhile are not visited.
    /// An entry removed by another thread may still be visited once if it was
    /// fetched just before the removal.
    pub(crate) fn for_each(&self, mut call: impl FnMut(V))
    where
        V: Clone,
    {
        let ids: Vec<usize> = lock(&self.entries).keys().copied().collect();
        for id in ids {
            let entry = lock(&self.entries).get(&id).cloned();
            if let Some(value) = entry {
                call(value);
            }
        }
    }
}
