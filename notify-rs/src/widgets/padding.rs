use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Constraints, EdgeInsets, Offset, Size};
use crate::widget::Widget;

/// Insets its child by `insets` on each side, including front and back.
pub struct Padding {
    insets: EdgeInsets,
    child: Rc<dyn Widget>,
}

impl Padding {
    pub fn new(insets: EdgeInsets, child: Rc<dyn Widget>) -> Padding {
        Padding { insets, child }
    }
}

impl Widget for Padding {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(PaddingElement {
            core: ElementCore::new(self),
        })
    }
}

struct PaddingElement {
    core: ElementCore,
}

impl Element for PaddingElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<Padding>().child.clone()]
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let insets = self.core.widget_as::<Padding>().insets;
        let child = self.core.layout_child(0, constraints.deflate(insets), true);
        self.core
            .set_child_offset(0, Offset::new(insets.left, insets.top, insets.back));
        constraints.constrain(Size::new(
            child.width + insets.horizontal(),
            child.height + insets.vertical(),
            child.depth + insets.depth(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::test_support::{layout_of, rc, sized};
    use crate::widgets::{Center, SizedBox};

    const W: Size = Size::flat(100.0, 100.0);

    #[test]
    fn padding_deflates_and_offsets() {
        assert_eq!(
            layout_of(
                W,
                rc(Center::new(rc(Padding::new(
                    EdgeInsets::all(10.0),
                    sized(30.0, 20.0)
                ))))
            ),
            "Center 100x100x0 @ 0,0,0\n  Padding 50x40x0 @ 25,30,0\n    SizedBox 30x20x0 @ 10,10,0\n"
        );
    }

    #[test]
    fn padding_with_depth() {
        let child = rc(SizedBox::new().width(10.0).height(10.0).depth(4.0));
        assert_eq!(
            layout_of(
                Size::new(100.0, 100.0, 50.0),
                rc(Center::new(rc(Padding::new(
                    EdgeInsets::ZERO.with_depth(5.0, 3.0),
                    child
                ))))
            ),
            "Center 100x100x50 @ 0,0,0\n  Padding 10x10x12 @ 45,45,19\n    SizedBox 10x10x4 @ 0,0,3\n"
        );
    }

    #[test]
    fn padding_in_a_zero_size_window() {
        assert_eq!(
            layout_of(
                Size::ZERO,
                rc(Padding::new(EdgeInsets::all(10.0), sized(5.0, 5.0)))
            ),
            "Padding 0x0x0 @ 0,0,0\n  SizedBox 0x0x0 @ 10,10,0\n"
        );
    }
}
