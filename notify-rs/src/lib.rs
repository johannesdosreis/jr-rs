//! Core of the `notify` GUI framework: widgets describe the interface, elements
//! are their live instances in a tree owned by an [`App`].
//!
//! Implement [`BuilderWidget`] for everyday widgets. A widget with a
//! [`Notifier`] is rebuilt, on the next [`App::update`], whenever the notifier
//! notifies, from any thread. Libraries that need their own kind of element
//! implement [`Widget::create_element`] and [`Element`], embedding an
//! [`ElementCore`]. Values are shared down the tree with
//! [`BuildContext::provide`] and read with [`BuildContext::get`].

mod app;
mod builder_element;
mod context;
mod element;
mod flags;
mod geometry;
mod key;
mod layout;
mod reconcile;
mod widget;

#[cfg(test)]
mod test_support;

pub use app::App;
pub use context::BuildContext;
pub use element::{Element, ElementCore};
pub use geometry::{Alignment, Axis, Constraints, EdgeInsets, Offset, Size};
pub use key::Key;
pub use layout::impossible_layout;
pub use reactive_rs::{Listenable, Listener, Notifier};
pub use widget::{BuilderWidget, Widget};
