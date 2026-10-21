use std::fmt::Write;
use std::rc::Rc;
use std::sync::Arc;

use crate::context::BuildContext;
use crate::element::Element;
use crate::flags::Signal;
use crate::reconcile::{destroy, update_child, update_children};
use crate::widget::Widget;

/// Owns the element tree and rebuilds the parts that changed.
pub struct App {
    pub(crate) root: Option<Box<dyn Element>>,
    signal: Arc<Signal>,
    #[cfg(test)]
    visited: usize,
}

impl App {
    pub fn new() -> Self {
        App {
            root: None,
            signal: Arc::new(Signal::new()),
            #[cfg(test)]
            visited: 0,
        }
    }

    /// Mounts `widget` as the root, or updates the current root with it, and
    /// builds whatever that changed.
    pub fn set_root(&mut self, widget: Rc<dyn Widget>) {
        self.root = update_child(self.root.take(), Some(widget), None, &self.signal);
        self.update();
    }

    /// Rebuilds the elements marked dirty since the last update, visiting only
    /// the branches that lead to them.
    pub fn update(&mut self) {
        self.signal.clear();
        let mut visited = 0;
        if let Some(root) = &mut self.root
            && root.core().flags.needs_visit()
        {
            visit(root, &self.signal, &mut visited);
        }
        #[cfg(test)]
        {
            self.visited = visited;
        }
    }

    /// One line per element, in pre-order, indented two spaces per level, with
    /// the widget's short type name and its key, if any.
    pub fn debug_tree(&self) -> String {
        let mut out = String::new();
        if let Some(root) = &self.root {
            write_tree(root.as_ref(), 0, &mut out);
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn last_visited(&self) -> usize {
        self.visited
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if let Some(root) = self.root.take() {
            destroy(root);
        }
    }
}

fn visit(element: &mut Box<dyn Element>, signal: &Arc<Signal>, visited: &mut usize) {
    *visited += 1;
    let flags = element.core().flags.clone();
    let rebuild = flags.take_dirty();
    flags.take_descendant_dirty();
    if rebuild {
        let widgets = element.build(&mut BuildContext::new());
        let old = std::mem::take(&mut element.core_mut().children);
        element.core_mut().children = update_children(old, widgets, &flags, signal);
    }
    for child in &mut element.core_mut().children {
        if child.core().flags.needs_visit() {
            visit(child, signal, visited);
        }
    }
}

fn write_tree(element: &dyn Element, depth: usize, out: &mut String) {
    let widget = element.widget();
    out.push_str(&"  ".repeat(depth));
    out.push_str(&short_type_name(widget.type_name()));
    if let Some(key) = widget.key() {
        let _ = write!(out, " #{key:?}");
    }
    out.push('\n');
    for child in element.children() {
        write_tree(child.as_ref(), depth + 1, out);
    }
}

/// Removes the module path of every segment: `a::b::C<d::E>` becomes `C<E>`.
fn short_type_name(full: &str) -> String {
    let mut out = String::new();
    let mut segment = String::new();
    let mut chars = full.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            segment.push(c);
        } else if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            segment.clear();
        } else {
            out.push_str(&segment);
            segment.clear();
            out.push(c);
        }
    }
    out.push_str(&segment);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Group, Leaf, Log, Probe, rc};

    fn probe_with_two_children(log: &Log) -> Rc<dyn Widget> {
        rc(Group::new(vec![rc(Probe::new("a", log).with_children(
            vec![rc(Probe::new("b", log)), rc(Probe::new("c", log))],
        ))]))
    }

    #[test]
    fn set_root_builds_the_whole_tree() {
        let mut app = App::new();
        app.set_root(rc(Group::new(vec![
            rc(Leaf::new()),
            rc(Group::new(vec![rc(Leaf::new())])),
        ])));
        assert_eq!(app.debug_tree(), "Group\n  Leaf\n  Group\n    Leaf\n");
    }

    #[test]
    fn debug_tree_shows_keys() {
        let mut app = App::new();
        app.set_root(rc(Group::new(vec![
            rc(Leaf::keyed("a")),
            rc(Leaf::keyed(1i32)),
        ])));
        assert_eq!(app.debug_tree(), "Group\n  Leaf #\"a\"\n  Leaf #1\n");
    }

    #[test]
    fn same_type_and_key_reuses_the_element() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(Probe::new("p", &log)));
        app.set_root(rc(Probe::new("p", &log)));
        assert_eq!(log.take(), ["create p", "build p", "build p"]);
    }

    #[test]
    fn different_type_replaces_the_element() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(Probe::new("p", &log)));
        app.set_root(rc(Leaf::new()));
        assert_eq!(log.take(), ["create p", "build p", "drop p"]);
    }

    #[test]
    fn different_key_replaces_the_element() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(Probe::new("p", &log).with_key(1i32)));
        app.set_root(rc(Probe::new("q", &log).with_key(2i32)));
        assert_eq!(
            log.take(),
            ["create p", "build p", "drop p", "create q", "build q"]
        );
    }

    #[test]
    fn same_instance_is_not_rebuilt() {
        let (log, mut app) = (Log::default(), App::new());
        let widget = rc(Probe::new("p", &log));
        app.set_root(widget.clone());
        app.set_root(widget);
        assert_eq!(log.take(), ["create p", "build p"]);
    }

    #[test]
    fn removing_an_element_drops_children_last_to_first_then_parent() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(probe_with_two_children(&log));
        log.take();
        app.set_root(rc(Group::new(vec![])));
        assert_eq!(log.take(), ["drop c", "drop b", "drop a"]);
    }

    #[test]
    fn dropping_the_app_drops_children_first() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(probe_with_two_children(&log));
        log.take();
        drop(app);
        assert_eq!(log.take(), ["drop c", "drop b", "drop a"]);
    }

    #[test]
    fn children_are_matched_by_position() {
        let (log, mut app) = (Log::default(), App::new());
        let pair = || {
            rc(Group::new(vec![
                rc(Probe::new("a", &log)),
                rc(Probe::new("b", &log)),
            ]))
        };
        app.set_root(pair());
        log.take();
        app.set_root(pair());
        assert_eq!(log.take(), ["build a", "build b"]);
        app.set_root(rc(Group::new(vec![rc(Probe::new("a", &log))])));
        assert_eq!(log.take(), ["drop b", "build a"]);
    }

    #[test]
    fn update_without_changes_visits_nothing() {
        let mut app = App::new();
        app.set_root(rc(Group::new(vec![rc(Leaf::new())])));
        app.update();
        assert_eq!(app.last_visited(), 0);
    }

    #[test]
    fn update_without_root_does_nothing() {
        let mut app = App::new();
        app.update();
        assert_eq!(app.debug_tree(), "");
    }

    #[test]
    fn deep_tree_builds_updates_and_drops() {
        let mut widget = rc(Leaf::new());
        for _ in 0..1000 {
            widget = rc(Group::new(vec![widget]));
        }
        let mut app = App::new();
        app.set_root(widget);
        assert_eq!(app.debug_tree().lines().count(), 1001);
        app.update();
        drop(app);
    }

    #[test]
    fn short_type_name_strips_module_paths() {
        assert_eq!(short_type_name("a::b::C<d::E, F>"), "C<E, F>");
    }
}
