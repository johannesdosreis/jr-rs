use std::sync::Arc;

use crate::subscription::Subscription;

pub type Listener = Arc<dyn Fn() + Send + Sync>;

pub trait Listenable: Send + Sync {
    fn add_listener(&self, listener: Listener) -> usize;
    fn remove_listener(&self, id: usize);

    #[must_use = "dropping the subscription removes the listener"]
    fn subscribe(&self, listener: Listener) -> impl Drop
    where
        Self: Sized,
    {
        let id = self.add_listener(listener);
        Subscription::new(self, id, |source: &Self, id| source.remove_listener(id))
    }
}

impl<L: Listenable + ?Sized> Listenable for Arc<L> {
    fn add_listener(&self, listener: Listener) -> usize {
        (**self).add_listener(listener)
    }

    fn remove_listener(&self, id: usize) {
        (**self).remove_listener(id)
    }
}
