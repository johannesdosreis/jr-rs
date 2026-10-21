use std::any::Any;
use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Alignment, Axis, Constraints, Offset, Size};
use crate::layout::impossible_layout;
use crate::widget::Widget;

/// How a stack constrains its non-positioned children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackFit {
    /// The stack's constraints, loosened.
    Loose,
    /// Tight at the largest size the stack allows.
    Expand,
    /// The stack's own constraints.
    Passthrough,
}

/// The parent data of `Positioned`, read by stack layouts: insets from each
/// side and explicit sizes, per axis.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct PositionedParentData {
    pub left: Option<f32>,
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub back: Option<f32>,
    pub front: Option<f32>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub depth: Option<f32>,
}

impl PositionedParentData {
    /// Start inset, end inset and explicit size along `axis`.
    fn along(&self, axis: Axis) -> (Option<f32>, Option<f32>, Option<f32>) {
        match axis {
            Axis::X => (self.left, self.right, self.width),
            Axis::Y => (self.top, self.bottom, self.height),
            Axis::Z => (self.back, self.front, self.depth),
        }
    }
}

/// Overlaps its children. Non-positioned children are aligned by `alignment`
/// and determine the stack's size; `Positioned` children are placed by their
/// insets.
pub struct Stack {
    alignment: Alignment,
    fit: StackFit,
    children: Vec<Rc<dyn Widget>>,
}

impl Stack {
    pub fn new(children: Vec<Rc<dyn Widget>>) -> Stack {
        Stack {
            alignment: Alignment::TOP_LEFT,
            fit: StackFit::Loose,
            children,
        }
    }

    pub fn alignment(mut self, alignment: Alignment) -> Stack {
        self.alignment = alignment;
        self
    }

    pub fn fit(mut self, fit: StackFit) -> Stack {
        self.fit = fit;
        self
    }
}

/// Places its child inside a `Stack` by insets and sizes. Outside a stack it
/// has no effect and its child passes through.
pub struct Positioned {
    data: PositionedParentData,
    child: Rc<dyn Widget>,
}

macro_rules! positioned_setters {
    ($($field:ident),*) => {
        $(pub fn $field(mut self, value: f32) -> Positioned {
            self.data.$field = Some(value);
            self
        })*
    };
}

impl Positioned {
    pub fn new(child: Rc<dyn Widget>) -> Positioned {
        Positioned {
            data: PositionedParentData::default(),
            child,
        }
    }

    positioned_setters!(left, top, right, bottom, back, front, width, height, depth);
}

impl Widget for Stack {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(StackElement {
            core: ElementCore::new(self),
        })
    }
}

impl Widget for Positioned {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(PositionedElement {
            core: ElementCore::new(self),
        })
    }
}

struct StackElement {
    core: ElementCore,
}

impl Element for StackElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<Stack>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let stack = self.core.widget_as::<Stack>();
        stack_layout(&mut self.core, stack.alignment, stack.fit, constraints)
    }
}

struct PositionedElement {
    core: ElementCore,
}

impl Element for PositionedElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<Positioned>().child.clone()]
    }

    fn parent_data(&self) -> Option<&dyn Any> {
        let widget: &dyn Any = &**self.core.widget();
        widget
            .downcast_ref::<Positioned>()
            .map(|positioned| &positioned.data as &dyn Any)
    }
}

fn stack_layout(
    core: &mut ElementCore,
    alignment: Alignment,
    fit: StackFit,
    c: Constraints,
) -> Size {
    let positioned: Vec<Option<PositionedParentData>> = core
        .children()
        .iter()
        .map(|child| {
            child
                .parent_data()
                .and_then(|data| data.downcast_ref::<PositionedParentData>())
                .copied()
        })
        .collect();

    let non_positioned = match fit {
        StackFit::Loose => c.loosen(),
        StackFit::Passthrough => c,
        StackFit::Expand => Axis::ALL.iter().fold(c, |result, &axis| {
            if c.has_bounded(axis) {
                result.with_axis(axis, c.max(axis), c.max(axis))
            } else {
                impossible_layout("Stack com StackFit::Expand precisa de eixos limitados");
                result.with_axis(axis, 0.0, f32::INFINITY)
            }
        }),
    };

    let mut sizes = vec![Size::ZERO; positioned.len()];
    let mut largest: Option<Size> = None;
    for (index, data) in positioned.iter().enumerate() {
        if data.is_none() {
            let child = core.layout_child(index, non_positioned, true);
            sizes[index] = child;
            let current = largest.unwrap_or(Size::ZERO);
            largest = Some(Size::new(
                current.width.max(child.width),
                current.height.max(child.height),
                current.depth.max(child.depth),
            ));
        }
    }
    let size = match largest {
        Some(largest) => c.constrain(largest),
        None => Axis::ALL.iter().fold(Size::ZERO, |size, &axis| {
            let value = if c.has_bounded(axis) {
                c.max(axis)
            } else {
                c.min(axis)
            };
            size.with_axis(axis, value)
        }),
    };

    for (index, data) in positioned.iter().enumerate() {
        match data {
            None => core.set_child_offset(index, alignment.offset_of(sizes[index], size)),
            Some(data) => {
                let child_constraints =
                    Axis::ALL
                        .iter()
                        .fold(Constraints::unbounded(), |result, &axis| {
                            match data.along(axis) {
                                (Some(start), Some(end), _) => {
                                    let extent = (size.along(axis) - start - end).max(0.0);
                                    result.with_axis(axis, extent, extent)
                                }
                                (_, _, Some(extent)) => result.with_axis(axis, extent, extent),
                                // Free in the plane, as in Flutter; never deeper
                                // than the stack, so flat UIs stay flat.
                                _ if axis == Axis::Z => result.with_axis(axis, 0.0, size.depth),
                                _ => result,
                            }
                        });
                let child = core.layout_child(index, child_constraints, true);
                let offset = Axis::ALL.iter().fold(Offset::ZERO, |offset, &axis| {
                    let value = match data.along(axis) {
                        (Some(start), _, _) => start,
                        (None, Some(end), _) => size.along(axis) - end - child.along(axis),
                        (None, None, _) => {
                            let free = size.along(axis) - child.along(axis);
                            free * (1.0 + alignment.along(axis)) / 2.0
                        }
                    };
                    offset.with_axis(axis, value)
                });
                core.set_child_offset(index, offset);
            }
        }
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{app_with, layout_of, rc, root_children, sized, unbounded_layout};
    use crate::widgets::{Center, SizedBox};

    const W: Size = Size::flat(300.0, 100.0);

    #[test]
    fn stack_sizes_to_largest_non_positioned_child() {
        assert!(
            layout_of(W, rc(Center::new(rc(Stack::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)])))))
                .contains("\n  Stack 50x40x0 @ 125,30,0\n    SizedBox 50x20x0 @ 0,0,0\n    SizedBox 30x40x0 @ 0,0,0\n")
        );
    }

    #[test]
    fn stack_alignment_positions_children() {
        let stack =
            Stack::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)]).alignment(Alignment::CENTER);
        assert!(
            layout_of(W, rc(Center::new(rc(stack))))
                .contains("\n    SizedBox 50x20x0 @ 0,10,0\n    SizedBox 30x40x0 @ 10,0,0\n")
        );
    }

    #[test]
    fn stack_fit_expand_makes_children_fill() {
        assert_eq!(
            layout_of(
                W,
                rc(Stack::new(vec![sized(50.0, 20.0)]).fit(StackFit::Expand))
            ),
            "Stack 300x100x0 @ 0,0,0\n  SizedBox 300x100x0 @ 0,0,0\n"
        );
    }

    #[test]
    fn stack_without_non_positioned_children_takes_biggest() {
        let stack = Stack::new(vec![rc(Positioned::new(sized(10.0, 10.0)).left(0.0))]);
        assert!(layout_of(W, rc(Center::new(rc(stack)))).contains("\n  Stack 300x100x0 @ 0,0,0\n"));
    }

    #[test]
    fn positioned_with_insets_and_sizes() {
        let app = app_with(
            W,
            rc(Stack::new(vec![
                sized(10.0, 10.0),
                rc(Positioned::new(sized(1.0, 1.0))
                    .left(10.0)
                    .right(20.0)
                    .top(5.0)
                    .height(30.0)),
                rc(Positioned::new(sized(20.0, 20.0)).right(10.0).bottom(10.0)),
            ])),
        );
        let children = root_children(&app);
        assert_eq!(
            (children[1].size(), children[1].offset()),
            (Size::flat(270.0, 30.0), Offset::new(10.0, 5.0, 0.0))
        );
        assert_eq!(
            (children[2].size(), children[2].offset()),
            (Size::flat(20.0, 20.0), Offset::new(270.0, 70.0, 0.0))
        );
    }

    #[test]
    fn positioned_in_depth() {
        let cube = || rc(SizedBox::new().width(10.0).height(10.0).depth(10.0));
        let app = app_with(
            Size::new(100.0, 100.0, 50.0),
            rc(Stack::new(vec![
                rc(Positioned::new(cube()).back(5.0)),
                rc(Positioned::new(cube()).front(5.0)),
            ])),
        );
        let zs: Vec<f32> = root_children(&app).iter().map(|c| c.offset().z).collect();
        assert_eq!(zs, [5.0, 35.0]);
    }

    #[test]
    fn positioned_outside_stack_passes_through() {
        assert!(
            layout_of(
                Size::flat(100.0, 100.0),
                rc(Center::new(
                    rc(Positioned::new(sized(10.0, 10.0)).left(3.0))
                ))
            )
            .contains("\n  Positioned 10x10x0 @ 45,45,0\n")
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Stack com StackFit::Expand precisa de eixos limitados")]
    fn stack_expand_needs_bounds() {
        unbounded_layout(
            W,
            rc(Stack::new(vec![sized(10.0, 10.0)]).fit(StackFit::Expand)),
        );
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn stack_expand_without_bounds_is_loose_in_release() {
        assert!(
            unbounded_layout(
                W,
                rc(Stack::new(vec![sized(10.0, 10.0)]).fit(StackFit::Expand))
            )
            .contains("\n    SizedBox 10x10x0 @ 0,0,0\n")
        );
    }

    #[test]
    fn positioned_without_depth_fits_the_stack_depth() {
        let fill = || {
            rc(Positioned::new(rc(SizedBox::expand()))
                .left(0.0)
                .top(0.0)
                .right(0.0)
                .bottom(0.0))
        };
        assert!(
            layout_of(W, rc(Stack::new(vec![fill()])))
                .contains("\n  Positioned 300x100x0 @ 0,0,0\n")
        );
        assert!(
            layout_of(Size::new(300.0, 100.0, 50.0), rc(Stack::new(vec![fill()])))
                .contains("\n  Positioned 300x100x50 @ 0,0,0\n")
        );
    }
}
