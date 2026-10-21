use crate::app::short_type_name;
use crate::element::{Element, ElementCore};
use crate::geometry::{Axis, Constraints, Offset, Size};

/// Reports a layout that cannot be satisfied (for example flexible children
/// along an unbounded axis). Panics in debug builds; in release builds it
/// returns and the caller applies its documented fallback.
pub fn impossible_layout(message: &str) {
    if cfg!(debug_assertions) {
        panic!("layout impossível: {message}");
    }
}

/// Lays out `element`, reusing its last size when it is clean and receives the
/// same constraints. Records whether it is a relayout boundary: an element
/// whose size cannot change without its constraints changing.
pub(crate) fn layout_element(
    element: &mut Box<dyn Element>,
    constraints: Constraints,
    parent_uses_size: bool,
) -> Size {
    let constraints = normalize_constraints(constraints);
    // An element exposing parent data is never a boundary: its parent's layout
    // depends on that data, so changes below must reach the parent.
    let boundary = (!parent_uses_size || element.sized_by_parent() || constraints.is_tight())
        && element.parent_data().is_none();
    let flags = element.core().flags.clone();
    flags.set_relayout_boundary(boundary);
    element.core_mut().parent_uses_size = parent_uses_size;

    if !flags.needs_layout() && element.core().constraints == Some(constraints) {
        if flags.descendant_needs_layout() {
            relayout_descendants(element);
        }
        return element.core().size;
    }

    flags.take_needs_layout();
    flags.take_descendant_needs_layout();
    let size = element.layout(constraints);
    let size = validate_size(element.as_ref(), size, constraints);
    let core = element.core_mut();
    core.constraints = Some(constraints);
    core.size = size;
    size
}

/// Lays out again the marked descendants of a clean element. Each marked child
/// is a relayout boundary, so it keeps its size and its constraints.
pub(crate) fn relayout_descendants(element: &mut Box<dyn Element>) {
    element.core().flags.take_descendant_needs_layout();
    for child in &mut element.core_mut().children {
        let flags = child.core().flags.clone();
        if !flags.needs_layout_visit() {
            continue;
        }
        if flags.needs_layout() {
            if let Some(constraints) = child.core().constraints {
                let parent_uses_size = child.core().parent_uses_size;
                layout_element(child, constraints, parent_uses_size);
            }
        } else {
            relayout_descendants(child);
        }
    }
}

/// Without children: the smallest size allowed. With children: each one gets
/// the same constraints and sits at the origin; the size is the largest child.
pub(crate) fn default_layout(core: &mut ElementCore, constraints: Constraints) -> Size {
    if core.children.is_empty() {
        return constraints.smallest();
    }
    let mut largest = Size::ZERO;
    for index in 0..core.children.len() {
        let size = core.layout_child(index, constraints, true);
        core.set_child_offset(index, Offset::ZERO);
        largest = Size::new(
            largest.width.max(size.width),
            largest.height.max(size.height),
            largest.depth.max(size.depth),
        );
    }
    constraints.constrain(largest)
}

fn normalize_constraints(constraints: Constraints) -> Constraints {
    let valid = Axis::ALL.iter().all(|&axis| {
        let (min, max) = (constraints.min(axis), constraints.max(axis));
        min.is_finite() && min >= 0.0 && !max.is_nan() && min <= max
    });
    if valid {
        return constraints;
    }
    if cfg!(debug_assertions) {
        panic!("restrições inválidas: {constraints:?}");
    }
    let fix = |value: f32| {
        if value.is_nan() || value < 0.0 {
            0.0
        } else {
            value
        }
    };
    Axis::ALL.iter().fold(constraints, |result, &axis| {
        let max = fix(constraints.max(axis));
        let min = fix(constraints.min(axis));
        let min = if min.is_finite() { min.min(max) } else { 0.0 };
        result.with_axis(axis, min, max)
    })
}

fn validate_size(element: &dyn Element, size: Size, constraints: Constraints) -> Size {
    let valid = Axis::ALL.iter().all(|&axis| {
        let value = size.along(axis);
        value.is_finite() && value >= constraints.min(axis) && value <= constraints.max(axis)
    });
    if valid {
        return size;
    }
    if cfg!(debug_assertions) {
        panic!(
            "{}: tamanho {size:?} fora das restrições {constraints:?}",
            short_type_name(element.widget().type_name())
        );
    }
    let finite = Axis::ALL.iter().fold(size, |size, &axis| {
        if size.along(axis).is_finite() {
            size
        } else {
            size.with_axis(axis, constraints.min(axis))
        }
    });
    constraints.constrain(finite)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::element::Element;
    use crate::geometry::{Constraints, Size};
    use crate::test_support::{Group, LayoutProbe, Log, rc};
    use reactive_rs::Notifier;
    use std::sync::Arc;
    use std::thread;

    fn probe(name: &'static str, log: &Log) -> LayoutProbe {
        LayoutProbe::new(name, log)
    }

    fn loose(width: f32, height: f32) -> Constraints {
        Constraints::loose(Size::flat(width, height))
    }

    fn tight(width: f32, height: f32) -> Constraints {
        Constraints::tight(Size::flat(width, height))
    }

    fn leaf_of(app: &App) -> &dyn Element {
        app.root.as_ref().unwrap().children()[0].children()[0].as_ref()
    }

    /// root (gives mid `mid`) → mid (gives leaf `leaf_constraints`) → leaf.
    fn chain(log: &Log, mid: Constraints, mid_probe: LayoutProbe, leaf: LayoutProbe) -> App {
        let mut app = App::new();
        app.set_size(Size::flat(100.0, 100.0));
        app.set_root(rc(probe("root", log)
            .child_constraints(mid)
            .children(vec![rc(mid_probe.children(vec![rc(leaf)]))])));
        log.take();
        app
    }

    #[test]
    fn update_lays_out_the_tree_with_window_constraints() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_size(Size::flat(100.0, 50.0));
        app.set_root(rc(probe("root", &log)
            .size(Size::flat(100.0, 50.0))
            .child_constraints(loose(100.0, 50.0))
            .children(vec![rc(probe("a", &log).size(Size::flat(30.0, 20.0)))])));
        assert_eq!(
            app.debug_layout(),
            "LayoutProbe 100x50x0 @ 0,0,0\n  LayoutProbe 30x20x0 @ 0,0,0\n"
        );
        assert_eq!(log.take(), ["layout root", "layout a"]);
    }

    #[test]
    fn no_layout_without_size() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_root(rc(probe("root", &log)));
        assert!(log.take().is_empty());
    }

    #[test]
    fn update_without_changes_does_not_lay_out() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_size(Size::flat(100.0, 50.0));
        app.set_root(rc(probe("root", &log).children(vec![rc(probe("a", &log))])));
        log.take();
        app.update();
        assert!(log.take().is_empty());
    }

    #[test]
    fn same_size_does_nothing_and_new_size_relayouts() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_size(Size::flat(100.0, 50.0));
        app.set_root(rc(probe("root", &log)));
        log.take();
        app.set_size(Size::flat(100.0, 50.0));
        assert!(!app.needs_update());
        app.update();
        assert!(log.take().is_empty());
        app.set_size(Size::flat(200.0, 50.0));
        assert!(app.needs_update());
        app.update();
        assert_eq!(log.take(), ["layout root"]);
    }

    #[test]
    fn needs_layout_propagates_to_the_nearest_boundary() {
        let log = Log::default();
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log).child_constraints(loose(50.0, 50.0)),
            probe("leaf", &log),
        );
        leaf_of(&app).core().mark_needs_layout();
        app.update();
        assert_eq!(log.take(), ["layout mid", "layout leaf"]);
    }

    #[test]
    fn parent_not_using_size_makes_a_boundary() {
        let log = Log::default();
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log)
                .child_constraints(loose(50.0, 50.0))
                .parent_uses_size(false),
            probe("leaf", &log),
        );
        leaf_of(&app).core().mark_needs_layout();
        app.update();
        assert_eq!(log.take(), ["layout leaf"]);
    }

    #[test]
    fn sized_by_parent_makes_a_boundary() {
        let log = Log::default();
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log).child_constraints(loose(50.0, 50.0)),
            probe("leaf", &log).sized_by_parent(true),
        );
        leaf_of(&app).core().mark_needs_layout();
        app.update();
        assert_eq!(log.take(), ["layout leaf"]);
    }

    #[test]
    fn clean_child_with_same_constraints_is_skipped() {
        let log = Log::default();
        let mut app = chain(
            &log,
            loose(100.0, 50.0),
            probe("mid", &log).child_constraints(loose(100.0, 50.0)),
            probe("leaf", &log),
        );
        app.root.as_ref().unwrap().children()[0]
            .core()
            .mark_needs_layout();
        app.update();
        assert_eq!(log.take(), ["layout root", "layout mid"]);
    }

    #[test]
    fn needs_layout_listener_works_from_another_thread() {
        let log = Log::default();
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log).child_constraints(loose(50.0, 50.0)),
            probe("leaf", &log),
        );
        let listener = leaf_of(&app).core().needs_layout_listener();
        thread::spawn(move || listener()).join().unwrap();
        assert!(app.needs_update());
        app.update();
        assert!(log.take().contains(&"layout leaf".to_string()));
    }

    #[test]
    fn default_layout_overlaps_children_and_takes_largest() {
        let (log, mut app) = (Log::default(), App::new());
        app.set_size(Size::flat(100.0, 100.0));
        app.set_root(rc(probe("root", &log)
            .child_constraints(loose(100.0, 100.0))
            .children(vec![rc(Group::new(vec![
                rc(probe("a", &log).size(Size::flat(10.0, 30.0))),
                rc(probe("b", &log).size(Size::flat(20.0, 5.0))),
            ]))])));
        assert!(app.debug_layout().contains("\n  Group 20x30x0 @ 0,0,0\n"));
    }

    fn invalid() -> Constraints {
        Constraints {
            min_width: 10.0,
            max_width: 5.0,
            ..loose(100.0, 100.0)
        }
    }

    fn app_with_child_constraints(log: &Log, c: Constraints, child: LayoutProbe) -> App {
        let mut app = App::new();
        app.set_size(Size::flat(100.0, 100.0));
        app.set_root(rc(probe("root", log)
            .child_constraints(c)
            .children(vec![rc(child)])));
        app
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "restrições inválidas")]
    fn invalid_constraints_panic_in_debug() {
        let log = Log::default();
        app_with_child_constraints(&log, invalid(), probe("a", &log).size(Size::flat(8.0, 8.0)));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn invalid_constraints_are_normalized_in_release() {
        let log = Log::default();
        let app = app_with_child_constraints(
            &log,
            invalid(),
            probe("a", &log).size(Size::flat(8.0, 8.0)),
        );
        assert_eq!(app.root.as_ref().unwrap().children()[0].size().width, 5.0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "fora das restrições")]
    fn size_outside_constraints_panics_in_debug() {
        let log = Log::default();
        app_with_child_constraints(
            &log,
            loose(100.0, 100.0),
            probe("a", &log).size(Size::flat(500.0, 500.0)).raw_size(),
        );
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn size_outside_constraints_is_clamped_in_release() {
        let log = Log::default();
        let app = app_with_child_constraints(
            &log,
            loose(100.0, 100.0),
            probe("a", &log).size(Size::flat(500.0, 500.0)).raw_size(),
        );
        assert_eq!(
            app.root.as_ref().unwrap().children()[0].size(),
            Size::flat(100.0, 100.0)
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "layout impossível: teste")]
    fn impossible_layout_panics_in_debug() {
        impossible_layout("teste");
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn impossible_layout_returns_in_release() {
        impossible_layout("teste");
    }

    #[test]
    fn rebuilt_element_is_laid_out_again() {
        let (log, notifier) = (Log::default(), Arc::new(Notifier::new()));
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log).child_constraints(loose(50.0, 50.0)),
            probe("leaf", &log).notifier(notifier.clone()),
        );
        notifier.notify_listeners();
        app.update();
        assert_eq!(log.take(), ["layout mid", "layout leaf"]);
    }

    #[test]
    fn rebuilt_element_with_parent_data_relayouts_its_parent() {
        let (log, notifier) = (Log::default(), Arc::new(Notifier::new()));
        let mut app = chain(
            &log,
            tight(50.0, 50.0),
            probe("mid", &log).child_constraints(tight(50.0, 50.0)),
            probe("leaf", &log)
                .notifier(notifier.clone())
                .parent_data(1),
        );
        notifier.notify_listeners();
        app.update();
        assert_eq!(log.take(), ["layout mid", "layout leaf"]);
    }
}
