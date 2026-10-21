use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use reactive_rs::Notifier;

use crate::builder_element::BuilderElement;
use crate::context::BuildContext;
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

/// The everyday widget: builds a single child. Without a notifier it is a
/// stateless composition; with one, it rebuilds whenever the notifier notifies.
///
/// The notifier and the data it guards live in fields of the widget, behind
/// `Arc`, and must be created by something that outlives the widget, such as
/// the application's top level or a model. A parent that creates them inside
/// its own `build` creates new ones on every rebuild. Calling [`set`](Self::set)
/// unconditionally inside `build` causes a rebuild on every frame.
pub trait BuilderWidget: Any {
    fn key(&self) -> Option<Key> {
        None
    }

    fn notifier(&self) -> Option<&Arc<Notifier>> {
        None
    }

    fn build(&self, ctx: &mut BuildContext<'_>) -> Rc<dyn Widget>;

    /// Runs `change`, then notifies. Panics if the widget has no notifier.
    fn set(&self, change: impl FnOnce())
    where
        Self: Sized,
    {
        change();
        match BuilderWidget::notifier(self) {
            Some(notifier) => notifier.notify_listeners(),
            None => panic!("BuilderWidget::set chamado em um widget sem notifier"),
        }
    }
}

impl<B: BuilderWidget> Widget for B {
    fn key(&self) -> Option<Key> {
        BuilderWidget::key(self)
    }

    fn notifier(&self) -> Option<&Arc<Notifier>> {
        BuilderWidget::notifier(self)
    }

    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(BuilderElement::<B>::new(self))
    }
}
