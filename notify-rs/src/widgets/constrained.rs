use std::marker::PhantomData;
use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Constraints, Offset, Size};
use crate::widget::Widget;

/// Imposes extra constraints on its child, combined with the parent's.
pub struct ConstrainedBox {
    constraints: Constraints,
    child: Option<Rc<dyn Widget>>,
}

impl ConstrainedBox {
    pub fn new(constraints: Constraints) -> ConstrainedBox {
        ConstrainedBox {
            constraints,
            child: None,
        }
    }

    pub fn child(mut self, child: Rc<dyn Widget>) -> ConstrainedBox {
        self.child = Some(child);
        self
    }
}

/// A box with an exact size on the axes that are set. Without a child it is a
/// fixed-size leaf.
pub struct SizedBox {
    width: Option<f32>,
    height: Option<f32>,
    depth: Option<f32>,
    child: Option<Rc<dyn Widget>>,
}

impl SizedBox {
    pub fn new() -> SizedBox {
        SizedBox {
            width: None,
            height: None,
            depth: None,
            child: None,
        }
    }

    /// As large as the parent allows on every axis.
    pub fn expand() -> SizedBox {
        SizedBox::new()
            .width(f32::INFINITY)
            .height(f32::INFINITY)
            .depth(f32::INFINITY)
    }

    /// As small as the parent allows on every axis.
    pub fn shrink() -> SizedBox {
        SizedBox::new().width(0.0).height(0.0).depth(0.0)
    }

    pub fn width(mut self, width: f32) -> SizedBox {
        self.width = Some(width);
        self
    }

    pub fn height(mut self, height: f32) -> SizedBox {
        self.height = Some(height);
        self
    }

    pub fn depth(mut self, depth: f32) -> SizedBox {
        self.depth = Some(depth);
        self
    }

    pub fn child(mut self, child: Rc<dyn Widget>) -> SizedBox {
        self.child = Some(child);
        self
    }
}

impl Default for SizedBox {
    fn default() -> SizedBox {
        SizedBox::new()
    }
}

/// A widget laid out as extra constraints around an optional child.
trait BoxWidget: Widget {
    fn extra(&self) -> Constraints;
    fn child_widget(&self) -> Option<&Rc<dyn Widget>>;
}

impl BoxWidget for ConstrainedBox {
    fn extra(&self) -> Constraints {
        self.constraints
    }

    fn child_widget(&self) -> Option<&Rc<dyn Widget>> {
        self.child.as_ref()
    }
}

impl BoxWidget for SizedBox {
    fn extra(&self) -> Constraints {
        let exact = |value: Option<f32>| value.map_or((0.0, f32::INFINITY), |v| (v, v));
        let (min_width, max_width) = exact(self.width);
        let (min_height, max_height) = exact(self.height);
        let (min_depth, max_depth) = exact(self.depth);
        Constraints {
            min_width,
            max_width,
            min_height,
            max_height,
            min_depth,
            max_depth,
        }
    }

    fn child_widget(&self) -> Option<&Rc<dyn Widget>> {
        self.child.as_ref()
    }
}

impl Widget for ConstrainedBox {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(BoxElement::<ConstrainedBox>::new(self))
    }
}

impl Widget for SizedBox {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(BoxElement::<SizedBox>::new(self))
    }
}

struct BoxElement<W> {
    core: ElementCore,
    _marker: PhantomData<fn() -> W>,
}

impl<W: BoxWidget> BoxElement<W> {
    fn new(widget: Rc<W>) -> Self {
        BoxElement {
            core: ElementCore::new(widget),
            _marker: PhantomData,
        }
    }
}

impl<W: BoxWidget> Element for BoxElement<W> {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core
            .widget_as::<W>()
            .child_widget()
            .cloned()
            .into_iter()
            .collect()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let inner = self.core.widget_as::<W>().extra().enforce(constraints);
        if self.core.children().is_empty() {
            return inner.constrain(Size::ZERO);
        }
        let size = self.core.layout_child(0, inner, true);
        self.core.set_child_offset(0, Offset::ZERO);
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::test_support::{layout_of, rc, sized};
    use crate::widgets::Center;

    const W: Size = Size::flat(100.0, 100.0);

    #[test]
    fn constrained_box_enforces_extra_constraints() {
        let c = Constraints::loose(Size::flat(50.0, 50.0));
        assert_eq!(
            layout_of(
                W,
                rc(Center::new(rc(
                    ConstrainedBox::new(c).child(sized(80.0, 80.0))
                )))
            ),
            "Center 100x100x0 @ 0,0,0\n  ConstrainedBox 50x50x0 @ 25,25,0\n    SizedBox 50x50x0 @ 0,0,0\n"
        );
    }

    #[test]
    fn constrained_box_without_child_takes_smallest() {
        let c = Constraints {
            min_width: 10.0,
            min_height: 20.0,
            ..Constraints::unbounded()
        };
        assert!(
            layout_of(W, rc(Center::new(rc(ConstrainedBox::new(c)))))
                .contains("\n  ConstrainedBox 10x20x0 @ 45,40,0\n")
        );
    }

    #[test]
    fn sized_box_has_exact_size() {
        assert!(
            layout_of(W, rc(Center::new(sized(30.0, 40.0))))
                .contains("\n  SizedBox 30x40x0 @ 35,30,0\n")
        );
    }

    #[test]
    fn sized_box_expand_and_shrink() {
        assert!(
            layout_of(W, rc(Center::new(rc(SizedBox::expand()))))
                .contains("\n  SizedBox 100x100x0 @ 0,0,0\n")
        );
        assert!(
            layout_of(W, rc(Center::new(rc(SizedBox::shrink()))))
                .contains("\n  SizedBox 0x0x0 @ 50,50,0\n")
        );
    }

    #[test]
    fn sized_box_constrains_its_child() {
        let layout = layout_of(
            W,
            rc(Center::new(rc(SizedBox::new()
                .width(30.0)
                .child(sized(10.0, 10.0))))),
        );
        assert!(layout.contains("\n  SizedBox 30x10x0 @ 35,45,0\n    SizedBox 30x10x0 @ 0,0,0\n"));
    }
}
