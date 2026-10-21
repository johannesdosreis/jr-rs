use std::any::Any;
use std::collections::{HashMap, HashSet};
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
    let mut element = widget.create_element();
    let core = element.core_mut();
    core.flags.attach(parent.cloned(), signal.clone());
    core.sync_subscription();
    core.flags.mark_local();
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
                core.sync_subscription();
                core.flags.mark_local();
                Some(old)
            } else {
                destroy(old);
                Some(mount(new, parent, signal))
            }
        }
    }
}

/// Matches the new widgets against the old children the way Flutter does:
/// compatible pairs at the top and the bottom are updated in place, and in the
/// middle old children are matched by key; unkeyed ones there are recreated.
pub(crate) fn update_children(
    old: Vec<Box<dyn Element>>,
    new: Vec<Rc<dyn Widget>>,
    parent: &Arc<Flags>,
    signal: &Arc<Signal>,
) -> Vec<Box<dyn Element>> {
    let mut old = Doomed(old.into_iter().map(Some).collect());
    check_unique_keys(&new);
    let compatible = |old: &Option<Box<dyn Element>>, new: &Rc<dyn Widget>| {
        old.as_ref()
            .is_some_and(|old| can_update(old.widget(), new))
    };
    let mut children = Doomed(Vec::with_capacity(new.len()));
    let (mut old_top, mut new_top) = (0, 0);
    let (mut old_bottom, mut new_bottom) = (old.0.len(), new.len());

    while old_top < old_bottom && new_top < new_bottom && compatible(&old.0[old_top], &new[new_top])
    {
        children.0.push(update_child(
            old.0[old_top].take(),
            Some(new[new_top].clone()),
            Some(parent),
            signal,
        ));
        old_top += 1;
        new_top += 1;
    }

    while old_top < old_bottom
        && new_top < new_bottom
        && compatible(&old.0[old_bottom - 1], &new[new_bottom - 1])
    {
        old_bottom -= 1;
        new_bottom -= 1;
    }

    let mut keyed = HashMap::new();
    for (offset, slot) in old.0[old_top..old_bottom].iter_mut().enumerate() {
        match slot.as_ref().and_then(|element| element.widget().key()) {
            Some(key) => {
                keyed.insert(key, old_top + offset);
            }
            None => slot.take().into_iter().for_each(destroy),
        }
    }

    for widget in &new[new_top..new_bottom] {
        let reused = widget
            .key()
            .and_then(|key| keyed.get(&key).copied())
            .filter(|&index| compatible(&old.0[index], widget))
            .and_then(|index| old.0[index].take());
        children.0.push(update_child(
            reused,
            Some(widget.clone()),
            Some(parent),
            signal,
        ));
    }

    for slot in &mut old.0[old_top..old_bottom] {
        slot.take().into_iter().for_each(destroy);
    }

    for (old_index, new_index) in (old_bottom..old.0.len()).zip(new_bottom..new.len()) {
        children.0.push(update_child(
            old.0[old_index].take(),
            Some(new[new_index].clone()),
            Some(parent),
            signal,
        ));
    }

    std::mem::take(&mut children.0)
        .into_iter()
        .flatten()
        .collect()
}

/// Elements in the middle of a reconciliation. If it panics, they are still
/// destroyed children first, last to first, instead of by the default drop
/// glue, which would drop each parent before its children.
struct Doomed(Vec<Option<Box<dyn Element>>>);

impl Drop for Doomed {
    fn drop(&mut self) {
        while let Some(slot) = self.0.pop() {
            slot.into_iter().for_each(destroy);
        }
    }
}

fn check_unique_keys(widgets: &[Rc<dyn Widget>]) {
    let mut seen = HashSet::new();
    for key in widgets.iter().filter_map(|widget| widget.key()) {
        if seen.contains(&key) {
            panic!("key duplicada entre irmãos: {key:?}");
        }
        seen.insert(key);
    }
}

/// Drops the children, last to first and recursively, before the element itself.
pub(crate) fn destroy(mut element: Box<dyn Element>) {
    let children = std::mem::take(&mut element.core_mut().children);
    children.into_iter().rev().for_each(destroy);
}

#[cfg(test)]
mod tests {
    use crate::app::App;
    use crate::element::Element;
    use crate::test_support::{Group, Leaf, Log, Probe, rc};
    use crate::widget::Widget;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::rc::Rc;

    /// A widget whose element cannot be created.
    struct Bomb;

    impl Widget for Bomb {
        fn create_element(self: Rc<Self>) -> Box<dyn Element> {
            panic!("create_element falhou");
        }
    }

    fn parent_with_two_children(log: &Log) -> Rc<dyn Widget> {
        rc(Probe::new("a", log)
            .with_children(vec![rc(Probe::new("b", log)), rc(Probe::new("c", log))]))
    }

    fn drops(log: &Log) -> Vec<String> {
        log.take()
            .into_iter()
            .filter(|entry| entry.starts_with("drop"))
            .collect()
    }

    fn keyed(log: &Log, name: &'static str, key: i32) -> Rc<dyn Widget> {
        rc(Probe::new(name, log).with_key(key))
    }

    fn group(children: Vec<Rc<dyn Widget>>) -> Rc<dyn Widget> {
        rc(Group::new(children))
    }

    fn lifecycle(log: &Log) -> Vec<String> {
        log.take()
            .into_iter()
            .filter(|entry| entry.starts_with("create") || entry.starts_with("drop"))
            .collect()
    }

    #[test]
    fn reordering_keeps_each_element_with_its_key() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![
            keyed(&log, "a", 1),
            keyed(&log, "b", 2),
            keyed(&log, "c", 3),
        ]));
        log.take();
        app.set_root(group(vec![
            keyed(&log, "c", 3),
            keyed(&log, "a", 1),
            keyed(&log, "b", 2),
        ]));
        assert_eq!(log.take(), ["build c", "build a", "build b"]);
        assert_eq!(
            app.debug_tree(),
            "Group\n  Probe #3\n  Probe #1\n  Probe #2\n"
        );
    }

    #[test]
    fn inserting_in_the_middle_creates_only_the_new_element() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![keyed(&log, "a", 1), keyed(&log, "c", 3)]));
        log.take();
        app.set_root(group(vec![
            keyed(&log, "a", 1),
            keyed(&log, "b", 2),
            keyed(&log, "c", 3),
        ]));
        assert_eq!(lifecycle(&log), ["create b"]);
    }

    #[test]
    fn removing_from_the_middle_drops_only_that_element() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![
            keyed(&log, "a", 1),
            keyed(&log, "b", 2),
            keyed(&log, "c", 3),
        ]));
        log.take();
        app.set_root(group(vec![keyed(&log, "a", 1), keyed(&log, "c", 3)]));
        assert_eq!(lifecycle(&log), ["drop b"]);
    }

    #[test]
    fn inserting_at_both_ends_creates_only_new_elements() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![keyed(&log, "b", 2)]));
        log.take();
        app.set_root(group(vec![
            keyed(&log, "a", 1),
            keyed(&log, "b", 2),
            keyed(&log, "c", 3),
        ]));
        assert_eq!(lifecycle(&log), ["create a", "create c"]);
    }

    #[test]
    fn unkeyed_children_in_the_middle_are_recreated() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![
            keyed(&log, "a", 1),
            rc(Probe::new("x", &log)),
            keyed(&log, "b", 2),
        ]));
        log.take();
        app.set_root(group(vec![
            keyed(&log, "b", 2),
            rc(Probe::new("y", &log)),
            keyed(&log, "a", 1),
        ]));
        assert_eq!(lifecycle(&log), ["drop x", "create y"]);
    }

    #[test]
    fn same_key_with_different_type_is_recreated() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![keyed(&log, "p", 1)]));
        log.take();
        app.set_root(group(vec![rc(Leaf::keyed(1i32))]));
        assert_eq!(lifecycle(&log), ["drop p"]);
    }

    #[test]
    #[should_panic(expected = "key duplicada entre irmãos")]
    fn duplicate_keys_panic() {
        App::new().set_root(group(vec![rc(Leaf::keyed(1i32)), rc(Leaf::keyed(1i32))]));
    }

    #[test]
    fn equal_values_of_different_key_types_are_distinct() {
        let mut app = App::new();
        app.set_root(group(vec![rc(Leaf::keyed(1u32)), rc(Leaf::keyed(1i64))]));
        assert_eq!(app.debug_tree(), "Group\n  Leaf #1\n  Leaf #1\n");
    }

    #[test]
    fn duplicate_key_panic_still_drops_children_first() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![parent_with_two_children(&log)]));
        log.take();
        let result = catch_unwind(AssertUnwindSafe(|| {
            app.set_root(group(vec![rc(Leaf::keyed(1i32)), rc(Leaf::keyed(1i32))]));
        }));
        assert!(result.is_err());
        assert_eq!(drops(&log), ["drop c", "drop b", "drop a"]);
    }

    #[test]
    fn create_element_panic_still_drops_children_first() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(group(vec![parent_with_two_children(&log)]));
        log.take();
        let result = catch_unwind(AssertUnwindSafe(|| {
            app.set_root(group(vec![parent_with_two_children(&log), rc(Bomb)]));
        }));
        assert!(result.is_err());
        assert_eq!(drops(&log), ["drop c", "drop b", "drop a"]);
    }
}
