/// Calls `unsubscription(source, target)` when dropped.
pub(crate) struct Subscription<S: Copy, T: Copy> {
    source: S,
    target: T,
    unsubscription: fn(S, T),
}

impl<S: Copy, T: Copy> Subscription<S, T> {
    pub(crate) fn new(source: S, target: T, unsubscription: fn(S, T)) -> Self {
        Subscription {
            source,
            target,
            unsubscription,
        }
    }
}

impl<S: Copy, T: Copy> Drop for Subscription<S, T> {
    fn drop(&mut self) {
        (self.unsubscription)(self.source, self.target);
    }
}
