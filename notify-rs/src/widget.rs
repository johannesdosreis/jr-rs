use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use reactive_rs::Notifier;

use crate::element::Element;
use crate::key::Key;

/// Immutable description of a part of the interface, shared as `Rc<dyn Widget>`.
pub trait Widget: Any {
    /// Distinguishes this widget among its siblings during reconciliation.
    fn key(&self) -> Option<Key> {
        None
    }

    /// When present, the framework listens to it while the element lives and
    /// rebuilds the element whenever it notifies.
    fn notifier(&self) -> Option<&Arc<Notifier>> {
        None
    }

    /// Creates the element that represents this widget in the tree, with an
    /// [`ElementCore`](crate::ElementCore) built from `self`.
    fn create_element(self: Rc<Self>) -> Box<dyn Element>;

    #[doc(hidden)]
    fn type_name(&self) -> &'static str {
        std::any::type_name::<Self>()
    }
}
