use std::rc::Rc;

use super::flex::{MainAxisAlignment, distribute};
use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Axis, Constraints, Offset, Size};
use crate::layout::impossible_layout;
use crate::widget::Widget;

/// How children are distributed within a run, or runs within the wrap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapAlignment {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

impl From<WrapAlignment> for MainAxisAlignment {
    fn from(alignment: WrapAlignment) -> MainAxisAlignment {
        match alignment {
            WrapAlignment::Start => MainAxisAlignment::Start,
            WrapAlignment::End => MainAxisAlignment::End,
            WrapAlignment::Center => MainAxisAlignment::Center,
            WrapAlignment::SpaceBetween => MainAxisAlignment::SpaceBetween,
            WrapAlignment::SpaceAround => MainAxisAlignment::SpaceAround,
            WrapAlignment::SpaceEvenly => MainAxisAlignment::SpaceEvenly,
        }
    }
}

/// Where a child sits within the cross extent of its run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrapCrossAlignment {
    Start,
    End,
    Center,
}

/// Lays its children out in runs along `direction` (X or Y), starting a new
/// run when the main axis is full. Shrinks to its content.
pub struct Wrap {
    direction: Axis,
    spacing: f32,
    run_spacing: f32,
    alignment: WrapAlignment,
    run_alignment: WrapAlignment,
    cross_alignment: WrapCrossAlignment,
    children: Vec<Rc<dyn Widget>>,
}

impl Wrap {
    pub fn new(children: Vec<Rc<dyn Widget>>) -> Wrap {
        Wrap {
            direction: Axis::X,
            spacing: 0.0,
            run_spacing: 0.0,
            alignment: WrapAlignment::Start,
            run_alignment: WrapAlignment::Start,
            cross_alignment: WrapCrossAlignment::Start,
            children,
        }
    }

    /// `Axis::X` or `Axis::Y`.
    pub fn direction(mut self, direction: Axis) -> Wrap {
        self.direction = direction;
        self
    }

    /// Space between children within a run.
    pub fn spacing(mut self, spacing: f32) -> Wrap {
        self.spacing = spacing;
        self
    }

    /// Space between runs.
    pub fn run_spacing(mut self, spacing: f32) -> Wrap {
        self.run_spacing = spacing;
        self
    }

    pub fn alignment(mut self, alignment: WrapAlignment) -> Wrap {
        self.alignment = alignment;
        self
    }

    pub fn run_alignment(mut self, alignment: WrapAlignment) -> Wrap {
        self.run_alignment = alignment;
        self
    }

    pub fn cross_axis_alignment(mut self, alignment: WrapCrossAlignment) -> Wrap {
        self.cross_alignment = alignment;
        self
    }
}

impl Widget for Wrap {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(WrapElement {
            core: ElementCore::new(self),
        })
    }
}

struct WrapElement {
    core: ElementCore,
}

impl Element for WrapElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<Wrap>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let wrap = self.core.widget_as::<Wrap>();
        wrap_layout(&mut self.core, &wrap, constraints)
    }
}

/// Children `start..end` share a run spanning `main` along the main axis and
/// `cross` along the cross axis.
struct Run {
    start: usize,
    end: usize,
    main: f32,
    cross: f32,
}

fn wrap_layout(core: &mut ElementCore, wrap: &Wrap, c: Constraints) -> Size {
    let main = if wrap.direction == Axis::Z {
        impossible_layout("Wrap só aceita Axis::X ou Axis::Y");
        Axis::X
    } else {
        wrap.direction
    };
    let cross = if main == Axis::X { Axis::Y } else { Axis::X };
    let child_constraints = Constraints::unbounded()
        .with_axis(main, 0.0, c.max(main))
        .with_axis(Axis::Z, 0.0, c.max(Axis::Z));

    let count = core.children().len();
    let sizes: Vec<Size> = (0..count)
        .map(|index| core.layout_child(index, child_constraints, true))
        .collect();

    let mut runs: Vec<Run> = Vec::new();
    for (index, size) in sizes.iter().enumerate() {
        let (child_main, child_cross) = (size.along(main), size.along(cross));
        match runs.last_mut() {
            Some(run) if run.main + wrap.spacing + child_main <= c.max(main) => {
                run.main += wrap.spacing + child_main;
                run.cross = run.cross.max(child_cross);
                run.end = index + 1;
            }
            _ => runs.push(Run {
                start: index,
                end: index + 1,
                main: child_main,
                cross: child_cross,
            }),
        }
    }

    let main_extent = runs.iter().map(|run| run.main).fold(0.0, f32::max);
    let cross_total = runs.iter().map(|run| run.cross).sum::<f32>()
        + wrap.run_spacing * runs.len().saturating_sub(1) as f32;
    let depth = sizes.iter().map(|size| size.depth).fold(0.0, f32::max);
    let size = c.constrain(
        Size::ZERO
            .with_axis(main, main_extent)
            .with_axis(cross, cross_total)
            .with_axis(Axis::Z, depth),
    );

    let (run_leading, run_between) = distribute(
        wrap.run_alignment.into(),
        size.along(cross) - cross_total,
        runs.len(),
        wrap.run_spacing,
    );
    let mut cross_position = run_leading;
    for run in &runs {
        let (leading, between) = distribute(
            wrap.alignment.into(),
            size.along(main) - run.main,
            run.end - run.start,
            wrap.spacing,
        );
        let mut main_position = leading;
        for (index, child) in sizes.iter().enumerate().take(run.end).skip(run.start) {
            let room = run.cross - child.along(cross);
            let within = match wrap.cross_alignment {
                WrapCrossAlignment::Start => 0.0,
                WrapCrossAlignment::End => room,
                WrapCrossAlignment::Center => room / 2.0,
            };
            let offset = Offset::ZERO
                .with_axis(main, main_position)
                .with_axis(cross, cross_position + within);
            core.set_child_offset(index, offset);
            main_position += child.along(main) + between;
        }
        cross_position += run.cross + run_between;
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::test_support::{app_with, layout_of, rc, root_children, sized};
    use crate::widgets::Center;

    const W: Size = Size::flat(100.0, 100.0);

    fn three() -> Vec<Rc<dyn Widget>> {
        vec![sized(40.0, 10.0), sized(40.0, 10.0), sized(40.0, 10.0)]
    }

    fn positions(app: &App) -> Vec<(f32, f32)> {
        root_children(app)
            .iter()
            .map(|c| (c.offset().x, c.offset().y))
            .collect()
    }

    #[test]
    fn wrap_breaks_lines() {
        let app = app_with(W, rc(Wrap::new(three())));
        assert_eq!(positions(&app), [(0.0, 0.0), (40.0, 0.0), (0.0, 10.0)]);
    }

    #[test]
    fn wrap_spacing_and_run_spacing() {
        let app = app_with(W, rc(Wrap::new(three()).spacing(10.0).run_spacing(5.0)));
        assert_eq!(positions(&app), [(0.0, 0.0), (50.0, 0.0), (0.0, 15.0)]);
    }

    #[test]
    fn wrap_alignment_centers_each_run() {
        let app = app_with(W, rc(Wrap::new(three()).alignment(WrapAlignment::Center)));
        assert_eq!(positions(&app), [(10.0, 0.0), (50.0, 0.0), (30.0, 10.0)]);
    }

    #[test]
    fn wrap_run_alignment_end() {
        let app = app_with(W, rc(Wrap::new(three()).run_alignment(WrapAlignment::End)));
        let ys: Vec<f32> = positions(&app).iter().map(|p| p.1).collect();
        assert_eq!(ys, [80.0, 80.0, 90.0]);
    }

    #[test]
    fn wrap_cross_alignment_inside_run() {
        let app = app_with(
            W,
            rc(Wrap::new(vec![sized(40.0, 10.0), sized(40.0, 30.0)])
                .cross_axis_alignment(WrapCrossAlignment::Center)),
        );
        let ys: Vec<f32> = positions(&app).iter().map(|p| p.1).collect();
        assert_eq!(ys, [10.0, 0.0]);
    }

    #[test]
    fn wrap_shrinks_to_content() {
        assert!(
            layout_of(W, rc(Center::new(rc(Wrap::new(three())))))
                .contains("\n  Wrap 80x20x0 @ 10,40,0\n")
        );
    }

    #[test]
    fn wrap_vertical_direction() {
        let app = app_with(
            Size::flat(100.0, 25.0),
            rc(Wrap::new(vec![
                sized(10.0, 10.0),
                sized(10.0, 10.0),
                sized(10.0, 10.0),
            ])
            .direction(Axis::Y)),
        );
        assert_eq!(positions(&app), [(0.0, 0.0), (0.0, 10.0), (10.0, 0.0)]);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Wrap só aceita Axis::X ou Axis::Y")]
    fn wrap_along_z_panics_in_debug() {
        app_with(W, rc(Wrap::new(three()).direction(Axis::Z)));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn wrap_along_z_uses_x_in_release() {
        let app = app_with(W, rc(Wrap::new(three()).direction(Axis::Z)));
        assert_eq!(positions(&app), [(0.0, 0.0), (40.0, 0.0), (0.0, 10.0)]);
    }
}
