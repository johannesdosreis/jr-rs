use std::sync::{Mutex, MutexGuard, PoisonError};

/// Ignores poisoning: listeners never run while a lock is held, and no
/// operation in this crate leaves the guarded data half-updated.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
