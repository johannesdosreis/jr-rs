use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Alignment, Axis, Constraints, Size};
use crate::widget::Widget;

/// Positions its child inside itself by `alignment`. On each axis its size is
/// the child's times the factor when one is set, else the maximum when the
/// axis is bounded, else the child's.
pub struct Align {
    alignment: Alignment,
    factors: [Option<f32>; 3],
    child: Rc<dyn Widget>,
}

impl Align {
    pub fn new(alignment: Alignment, child: Rc<dyn Widget>) -> Align {
        Align {
            alignment,
            factors: [None; 3],
            child,
        }
    }

    pub fn width_factor(mut self, factor: f32) -> Align {
        self.factors[Axis::X as usize] = Some(factor);
        self
    }

    pub fn height_factor(mut self, factor: f32) -> Align {
        self.factors[Axis::Y as usize] = Some(factor);
        self
    }

    pub fn depth_factor(mut self, factor: f32) -> Align {
        self.factors[Axis::Z as usize] = Some(factor);
        self
    }
}

/// Centers its child; an [`Align`] with [`Alignment::CENTER`].
pub struct Center {
    child: Rc<dyn Widget>,
}

impl Center {
    pub fn new(child: Rc<dyn Widget>) -> Center {
        Center { child }
    }
}

impl Widget for Align {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(AlignElement {
            core: ElementCore::new(self),
        })
    }
}

impl Widget for Center {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(CenterElement {
            core: ElementCore::new(self),
        })
    }
}

struct AlignElement {
    core: ElementCore,
}

impl Element for AlignElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<Align>().child.clone()]
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let widget = self.core.widget_as::<Align>();
        align_layout(
            &mut self.core,
            widget.alignment,
            widget.factors,
            constraints,
        )
    }
}

struct CenterElement {
    core: ElementCore,
}

impl Element for CenterElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<Center>().child.clone()]
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        align_layout(&mut self.core, Alignment::CENTER, [None; 3], constraints)
    }
}

fn align_layout(
    core: &mut ElementCore,
    alignment: Alignment,
    factors: [Option<f32>; 3],
    constraints: Constraints,
) -> Size {
    let child = core.layout_child(0, constraints.loosen(), true);
    let size = Axis::ALL.iter().fold(Size::ZERO, |size, &axis| {
        let value = match factors[axis as usize] {
            Some(factor) => child.along(axis) * factor,
            None if constraints.has_bounded(axis) => constraints.max(axis),
            None => child.along(axis),
        };
        size.with_axis(axis, value)
    });
    let size = constraints.constrain(size);
    core.set_child_offset(0, alignment.offset_of(child, size));
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{layout_of, rc, sized};

    #[test]
    fn align_positions_child() {
        assert_eq!(
            layout_of(
                Size::flat(100.0, 50.0),
                rc(Align::new(Alignment::BOTTOM_RIGHT, sized(20.0, 10.0)))
            ),
            "Align 100x50x0 @ 0,0,0\n  SizedBox 20x10x0 @ 80,40,0\n"
        );
    }

    #[test]
    fn align_factors_size_to_child() {
        let align = Align::new(Alignment::TOP_LEFT, sized(10.0, 10.0))
            .width_factor(2.0)
            .height_factor(3.0);
        assert_eq!(
            layout_of(Size::flat(100.0, 100.0), rc(Center::new(rc(align)))),
            "Center 100x100x0 @ 0,0,0\n  Align 20x30x0 @ 40,35,0\n    SizedBox 10x10x0 @ 0,0,0\n"
        );
    }
}
