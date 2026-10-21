use std::any::{Any, type_name};
use std::rc::Rc;
use std::sync::Arc;

use reactive_rs::{Listenable, Listener, Notifier};

use crate::context::BuildContext;
use crate::flags::Flags;
use crate::widget::Widget;

/// A live instance of a widget in the tree. Implementors embed an
/// [`ElementCore`], which the framework manages, and only decide how to build
/// their children.
pub trait Element {
    fn core(&self) -> &ElementCore;
    fn core_mut(&mut self) -> &mut ElementCore;

    /// Returns the child widgets, in order. The framework reconciles them.
    fn build(&mut self, ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>>;

    fn widget(&self) -> &Rc<dyn Widget> {
        self.core().widget()
    }

    fn children(&self) -> &[Box<dyn Element>] {
        self.core().children()
    }

    fn mark_dirty(&self) {
        self.core().mark_dirty()
    }
}

/// The part of every element that the framework manages: its widget, its
/// children and its dirty state.
pub struct ElementCore {
    pub(crate) widget: Rc<dyn Widget>,
    pub(crate) children: Vec<Box<dyn Element>>,
    pub(crate) flags: Arc<Flags>,
    pub(crate) subscription: Option<(Arc<Notifier>, usize)>,
}

impl ElementCore {
    pub fn new(widget: Rc<dyn Widget>) -> Self {
        ElementCore {
            widget,
            children: Vec::new(),
            flags: Flags::new(),
            subscription: None,
        }
    }

    pub fn widget(&self) -> &Rc<dyn Widget> {
        &self.widget
    }

    /// The current widget as its concrete type. Panics if it is not a `W`,
    /// which never happens for an element's own widget type.
    pub fn widget_as<W: Widget>(&self) -> Rc<W> {
        let widget: Rc<dyn Any> = self.widget.clone();
        widget.downcast::<W>().unwrap_or_else(|_| {
            panic!(
                "widget_as::<{}>: o widget é {}",
                type_name::<W>(),
                self.widget.type_name()
            )
        })
    }

    pub fn children(&self) -> &[Box<dyn Element>] {
        &self.children
    }

    /// Schedules a rebuild of this element. Safe from any thread through
    /// [`dirty_listener`](Self::dirty_listener).
    pub fn mark_dirty(&self) {
        self.flags.mark_dirty();
    }

    /// A listener that marks this element dirty, ready for `add_listener`.
    pub fn dirty_listener(&self) -> Listener {
        let flags = self.flags.clone();
        Arc::new(move || flags.mark_dirty())
    }

    /// Listens to the current widget's notifier, moving the subscription when
    /// the notifier changed.
    pub(crate) fn sync_subscription(&mut self) {
        let current = self.widget.notifier().cloned();
        let unchanged = match (&self.subscription, &current) {
            (Some((subscribed, _)), Some(current)) => Arc::ptr_eq(subscribed, current),
            (None, None) => true,
            _ => false,
        };
        if unchanged {
            return;
        }
        self.unsubscribe();
        if let Some(notifier) = current {
            let id = notifier.add_listener(self.dirty_listener());
            self.subscription = Some((notifier, id));
        }
    }

    fn unsubscribe(&mut self) {
        if let Some((notifier, id)) = self.subscription.take() {
            notifier.remove_listener(id);
        }
    }
}

impl Drop for ElementCore {
    fn drop(&mut self) {
        self.unsubscribe();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::Key;
    use crate::test_support::{Group, Leaf, rc};

    #[test]
    fn widget_as_returns_the_concrete_widget() {
        let core = ElementCore::new(rc(Leaf::keyed("x")));
        assert_eq!(core.widget_as::<Leaf>().key, Some(Key::from("x")));
    }

    #[test]
    #[should_panic(expected = "widget_as")]
    fn widget_as_panics_on_wrong_type() {
        ElementCore::new(rc(Leaf::new())).widget_as::<Group>();
    }

    #[test]
    fn type_name_is_the_concrete_type() {
        assert!(rc(Leaf::new()).type_name().ends_with("Leaf"));
    }

    #[test]
    fn dirty_listener_marks_the_core() {
        let core = ElementCore::new(rc(Leaf::new()));
        (core.dirty_listener())();
        assert!(core.flags.take_dirty());
    }

    #[test]
    fn dirty_listener_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        assert_send_sync(&ElementCore::new(rc(Leaf::new())).dirty_listener());
    }
}
