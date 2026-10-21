use std::sync::Arc;

use crate::subscription::Subscription;

pub type ValueListener<T> = Arc<dyn Fn(T) + Send + Sync>;

pub trait ValueListenable<T>: Send + Sync {
    fn add_listener(&self, listener: ValueListener<T>) -> usize;
    fn remove_listener(&self, id: usize);

    #[must_use = "dropping the subscription removes the listener"]
    fn subscribe(&self, listener: ValueListener<T>) -> impl Drop
    where
        Self: Sized,
    {
        let id = self.add_listener(listener);
        Subscription::new(self, id, |source: &Self, id| source.remove_listener(id))
    }
}
