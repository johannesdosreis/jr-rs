use std::any::Any;
use std::rc::Rc;
use std::sync::Arc;

use crate::element::Element;
use crate::flags::{Flags, Signal};
use crate::widget::Widget;

/// Whether an element holding `old` can take `new`: same concrete type and same key.
pub(crate) fn can_update(old: &Rc<dyn Widget>, new: &Rc<dyn Widget>) -> bool {
    let (old_any, new_any): (&dyn Any, &dyn Any) = (&**old, &**new);
    old_any.type_id() == new_any.type_id() && old.key() == new.key()
}

/// Creates the element for `widget` and attaches it to the tree. It is marked
/// dirty so the ongoing visit builds it.
pub(crate) fn mount(
    widget: Rc<dyn Widget>,
    parent: Option<&Arc<Flags>>,
    signal: &Arc<Signal>,
) -> Box<dyn Element> {
    let element = widget.create_element();
    let flags = &element.core().flags;
    flags.attach(parent.cloned(), signal.clone());
    flags.mark_local();
    element
}

pub(crate) fn update_child(
    old: Option<Box<dyn Element>>,
    new: Option<Rc<dyn Widget>>,
    parent: Option<&Arc<Flags>>,
    signal: &Arc<Signal>,
) -> Option<Box<dyn Element>> {
    match (old, new) {
        (None, None) => None,
        (Some(old), None) => {
            destroy(old);
            None
        }
        (None, Some(new)) => Some(mount(new, parent, signal)),
        (Some(mut old), Some(new)) => {
            if Rc::ptr_eq(old.widget(), &new) {
                Some(old)
            } else if can_update(old.widget(), &new) {
                let core = old.core_mut();
                core.widget = new;
                core.flags.mark_local();
                Some(old)
            } else {
                destroy(old);
                Some(mount(new, parent, signal))
            }
        }
    }
}

pub(crate) fn update_children(
    old: Vec<Box<dyn Element>>,
    new: Vec<Rc<dyn Widget>>,
    parent: &Arc<Flags>,
    signal: &Arc<Signal>,
) -> Vec<Box<dyn Element>> {
    let mut old = old.into_iter();
    let mut children = Vec::with_capacity(new.len());
    for widget in new {
        children.extend(update_child(old.next(), Some(widget), Some(parent), signal));
    }
    old.for_each(destroy);
    children
}

/// Drops the children, last to first and recursively, before the element itself.
pub(crate) fn destroy(mut element: Box<dyn Element>) {
    let children = std::mem::take(&mut element.core_mut().children);
    children.into_iter().rev().for_each(destroy);
}
