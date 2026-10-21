use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

type Wake = Arc<dyn Fn() + Send + Sync>;

/// Tells the backend that the tree needs an update. Shared by every element of an App.
pub(crate) struct Signal {
    pending: AtomicBool,
    wake: Mutex<Option<Wake>>,
}

impl Signal {
    pub(crate) fn new() -> Signal {
        Signal {
            pending: AtomicBool::new(false),
            wake: Mutex::new(None),
        }
    }

    /// Calls the wake callback only on the transition from "nothing pending" to "pending".
    pub(crate) fn request(&self) {
        if !self.pending.swap(true, SeqCst) {
            self.wake();
        }
    }

    pub(crate) fn clear(&self) {
        self.pending.store(false, SeqCst);
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending.load(SeqCst)
    }

    /// Replaces the wake callback, calling it at once if an update is already pending.
    pub(crate) fn set_wake(&self, wake: Wake) {
        let old = self
            .wake
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(wake);
        if self.is_pending() {
            self.wake();
        }
        // The old callback's destructor is user code and may mark an element
        // dirty, which locks `wake` again: drop it only after the lock is released.
        drop(old);
    }

    fn wake(&self) {
        let wake = self
            .wake
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}

/// Dirty state of one element. `descendant_dirty` on every ancestor of a dirty
/// element lets an update walk only the branches that lead to dirty elements.
pub(crate) struct Flags {
    dirty: AtomicBool,
    descendant_dirty: AtomicBool,
    needs_layout: AtomicBool,
    descendant_needs_layout: AtomicBool,
    relayout_boundary: AtomicBool,
    parent: OnceLock<Arc<Flags>>,
    signal: OnceLock<Arc<Signal>>,
}

impl Flags {
    pub(crate) fn new() -> Arc<Flags> {
        Arc::new(Flags {
            dirty: AtomicBool::new(false),
            descendant_dirty: AtomicBool::new(false),
            needs_layout: AtomicBool::new(true),
            descendant_needs_layout: AtomicBool::new(false),
            relayout_boundary: AtomicBool::new(false),
            parent: OnceLock::new(),
            signal: OnceLock::new(),
        })
    }

    pub(crate) fn attach(&self, parent: Option<Arc<Flags>>, signal: Arc<Signal>) {
        if let Some(parent) = parent {
            let _ = self.parent.set(parent);
        }
        let _ = self.signal.set(signal);
    }

    /// Safe from any thread: marks this element, marks its ancestors up to the
    /// first one already marked, then requests an update.
    pub(crate) fn mark_dirty(&self) {
        self.dirty.store(true, SeqCst);
        let mut ancestor = self.parent.get();
        while let Some(flags) = ancestor {
            if flags.descendant_dirty.swap(true, SeqCst) {
                break;
            }
            ancestor = flags.parent.get();
        }
        if let Some(signal) = self.signal.get() {
            signal.request();
        }
    }

    /// Marks only this element; its parent is being visited and checks its children next.
    pub(crate) fn mark_local(&self) {
        self.dirty.store(true, SeqCst);
    }

    /// Marks this element and its ancestors strictly below `stop`, without
    /// requesting an update. Used while `stop` is being visited.
    pub(crate) fn mark_within(&self, stop: &Arc<Flags>) {
        self.dirty.store(true, SeqCst);
        let mut ancestor = self.parent.get();
        while let Some(flags) = ancestor {
            if Arc::ptr_eq(flags, stop) || flags.descendant_dirty.swap(true, SeqCst) {
                break;
            }
            ancestor = flags.parent.get();
        }
    }

    /// Safe from any thread: marks this element for layout, then its ancestors
    /// up to the nearest relayout boundary (whose size cannot change), sets
    /// `descendant_needs_layout` above that boundary and requests an update.
    pub(crate) fn mark_needs_layout(&self) {
        self.mark_needs_layout_quiet();
        if let Some(signal) = self.signal.get() {
            signal.request();
        }
    }

    /// Like `mark_needs_layout`, without requesting an update: used by the
    /// framework during an update.
    pub(crate) fn mark_needs_layout_quiet(&self) {
        let mut node = self;
        loop {
            if node.needs_layout.swap(true, SeqCst) {
                return;
            }
            match node.parent.get() {
                Some(parent) if !node.relayout_boundary.load(SeqCst) => node = parent,
                _ => break,
            }
        }
        let mut ancestor = node.parent.get();
        while let Some(flags) = ancestor {
            if flags.descendant_needs_layout.swap(true, SeqCst) {
                break;
            }
            ancestor = flags.parent.get();
        }
    }

    pub(crate) fn set_relayout_boundary(&self, boundary: bool) {
        self.relayout_boundary.store(boundary, SeqCst);
    }

    pub(crate) fn needs_layout(&self) -> bool {
        self.needs_layout.load(SeqCst)
    }

    pub(crate) fn descendant_needs_layout(&self) -> bool {
        self.descendant_needs_layout.load(SeqCst)
    }

    pub(crate) fn needs_layout_visit(&self) -> bool {
        self.needs_layout() || self.descendant_needs_layout()
    }

    pub(crate) fn take_needs_layout(&self) -> bool {
        self.needs_layout.swap(false, SeqCst)
    }

    pub(crate) fn take_descendant_needs_layout(&self) -> bool {
        self.descendant_needs_layout.swap(false, SeqCst)
    }

    #[allow(dead_code)]
    pub(crate) fn parent(&self) -> Option<&Arc<Flags>> {
        self.parent.get()
    }

    pub(crate) fn take_dirty(&self) -> bool {
        self.dirty.swap(false, SeqCst)
    }

    pub(crate) fn take_descendant_dirty(&self) -> bool {
        self.descendant_dirty.swap(false, SeqCst)
    }

    pub(crate) fn needs_visit(&self) -> bool {
        self.is_dirty() || self.is_descendant_dirty()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty.load(SeqCst)
    }

    pub(crate) fn is_descendant_dirty(&self) -> bool {
        self.descendant_dirty.load(SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    fn chain(n: usize) -> (Arc<Signal>, Vec<Arc<Flags>>) {
        let signal = Arc::new(Signal::new());
        let mut flags: Vec<Arc<Flags>> = Vec::new();
        for _ in 0..n {
            let f = Flags::new();
            f.attach(flags.last().cloned(), signal.clone());
            flags.push(f);
        }
        (signal, flags)
    }

    fn counter(signal: &Signal) -> Arc<AtomicUsize> {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        signal.set_wake(Arc::new(move || {
            c.fetch_add(1, SeqCst);
        }));
        count
    }

    #[test]
    fn mark_dirty_sets_dirty_and_ancestor_flags() {
        let (_signal, f) = chain(3);
        f[2].mark_dirty();
        assert!(f[2].is_dirty() && f[1].is_descendant_dirty() && f[0].is_descendant_dirty());
        assert!(!f[0].is_dirty() && !f[2].is_descendant_dirty());
    }

    #[test]
    fn mark_dirty_stops_at_first_marked_ancestor() {
        let (_signal, f) = chain(3);
        f[2].mark_dirty();
        f[0].take_descendant_dirty();
        f[2].mark_dirty();
        assert!(!f[0].is_descendant_dirty());
    }

    #[test]
    fn mark_dirty_requests_an_update() {
        let (signal, f) = chain(2);
        f[1].mark_dirty();
        assert!(signal.is_pending());
    }

    #[test]
    fn signal_wakes_once_per_transition() {
        let signal = Signal::new();
        let count = counter(&signal);
        signal.request();
        signal.request();
        signal.request();
        assert_eq!(count.load(SeqCst), 1);
        signal.clear();
        signal.request();
        assert_eq!(count.load(SeqCst), 2);
    }

    #[test]
    fn set_wake_with_pending_request_calls_it_at_once() {
        let signal = Signal::new();
        signal.request();
        let count = counter(&signal);
        assert_eq!(count.load(SeqCst), 1);
    }

    #[test]
    fn mark_local_neither_propagates_nor_signals() {
        let (signal, f) = chain(2);
        f[1].mark_local();
        assert!(f[1].is_dirty());
        assert!(!f[0].is_descendant_dirty() && !signal.is_pending());
    }

    #[test]
    fn mark_within_stops_below_stop_and_does_not_signal() {
        let (signal, f) = chain(4);
        f[3].mark_within(&f[1]);
        assert!(f[3].is_dirty() && f[2].is_descendant_dirty());
        assert!(!f[1].is_descendant_dirty() && !f[0].is_descendant_dirty() && !signal.is_pending());
    }

    #[test]
    fn take_and_needs_visit() {
        let (_signal, f) = chain(2);
        assert!(!f[0].needs_visit());
        f[1].mark_dirty();
        assert!(f[0].needs_visit() && f[1].needs_visit());
        assert!(f[1].take_dirty() && !f[1].take_dirty());
        assert!(f[0].take_descendant_dirty() && !f[0].needs_visit());
    }

    struct MarkOnDrop(Arc<Flags>);

    impl Drop for MarkOnDrop {
        fn drop(&mut self) {
            self.0.mark_dirty();
        }
    }

    #[test]
    fn replacing_wake_drops_the_old_callback_outside_the_lock() {
        let (done, result) = mpsc::channel();
        thread::spawn(move || {
            let (signal, f) = chain(1);
            let guard = MarkOnDrop(f[0].clone());
            signal.set_wake(Arc::new(move || {
                let _ = &guard;
            }));
            let count = counter(&signal);
            done.send(count.load(SeqCst)).unwrap();
        });
        assert_eq!(result.recv_timeout(Duration::from_secs(5)), Ok(1));
    }

    fn clear_layout(flags: &[Arc<Flags>]) {
        for f in flags {
            f.take_needs_layout();
        }
    }

    #[test]
    fn new_flags_need_layout() {
        let (_signal, f) = chain(1);
        assert!(f[0].needs_layout());
    }

    #[test]
    fn needs_layout_propagates_up_to_the_boundary() {
        let (_signal, f) = chain(4);
        clear_layout(&f);
        f[1].set_relayout_boundary(true);
        f[3].mark_needs_layout_quiet();
        assert!(f[3].needs_layout() && f[2].needs_layout() && f[1].needs_layout());
        assert!(!f[0].needs_layout() && f[0].descendant_needs_layout());
        assert!(!f[1].descendant_needs_layout());
    }

    #[test]
    fn root_acts_as_boundary() {
        let (_signal, f) = chain(2);
        clear_layout(&f);
        f[1].mark_needs_layout_quiet();
        assert!(f[0].needs_layout());
    }

    #[test]
    fn marking_stops_at_an_already_marked_element() {
        let (_signal, f) = chain(3);
        clear_layout(&f);
        f[1].mark_needs_layout_quiet();
        f[0].take_needs_layout();
        f[2].mark_needs_layout_quiet();
        assert!(!f[0].needs_layout());
    }

    #[test]
    fn quiet_marking_does_not_signal() {
        let (signal, f) = chain(2);
        clear_layout(&f);
        f[1].mark_needs_layout_quiet();
        assert!(!signal.is_pending());
        clear_layout(&f);
        f[1].mark_needs_layout();
        assert!(signal.is_pending());
    }

    #[test]
    fn take_and_visit_for_layout() {
        let (_signal, f) = chain(2);
        assert!(f[1].take_needs_layout() && !f[1].needs_layout_visit());
        f[1].set_relayout_boundary(true);
        f[1].mark_needs_layout_quiet();
        assert!(f[0].needs_layout_visit() && f[0].take_descendant_needs_layout());
        assert!(
            f[1].parent()
                .is_some_and(|parent| Arc::ptr_eq(parent, &f[0]))
        );
    }
}
