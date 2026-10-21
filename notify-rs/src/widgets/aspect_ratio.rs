use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Axis, Constraints, Offset, Size};
use crate::layout::impossible_layout;
use crate::widget::Widget;

/// Sizes its child so that width / height equals `ratio` in the plane; depth
/// passes through.
pub struct AspectRatio {
    ratio: f32,
    child: Rc<dyn Widget>,
}

impl AspectRatio {
    pub fn new(ratio: f32, child: Rc<dyn Widget>) -> AspectRatio {
        AspectRatio { ratio, child }
    }
}

impl Widget for AspectRatio {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(AspectRatioElement {
            core: ElementCore::new(self),
        })
    }
}

struct AspectRatioElement {
    core: ElementCore,
}

impl Element for AspectRatioElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        vec![self.core.widget_as::<AspectRatio>().child.clone()]
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let ratio = self.core.widget_as::<AspectRatio>().ratio;
        let Some((width, height)) = plane_size(ratio, constraints) else {
            impossible_layout("AspectRatio precisa de largura ou altura limitada");
            self.core
                .layout_child(0, Constraints::tight(constraints.smallest()), true);
            return constraints.smallest();
        };
        let child_constraints = Constraints::tight(Size::flat(width, height)).with_axis(
            Axis::Z,
            constraints.min_depth,
            constraints.max_depth,
        );
        let child = self.core.layout_child(0, child_constraints, true);
        self.core.set_child_offset(0, Offset::ZERO);
        constraints.constrain(Size::new(width, height, child.depth))
    }
}

/// Flutter's aspect ratio algorithm in the XY plane; `None` when neither the
/// width nor the height is bounded.
fn plane_size(ratio: f32, c: Constraints) -> Option<(f32, f32)> {
    if c.min_width == c.max_width && c.min_height == c.max_height {
        return Some((c.min_width, c.min_height));
    }
    let (mut width, mut height);
    if c.max_width.is_finite() {
        width = c.max_width;
        height = width / ratio;
    } else {
        height = c.max_height;
        if !height.is_finite() {
            return None;
        }
        width = height * ratio;
    }
    if width > c.max_width {
        width = c.max_width;
        height = width / ratio;
    }
    if height > c.max_height {
        height = c.max_height;
        width = height * ratio;
    }
    if width < c.min_width {
        width = c.min_width;
        height = width / ratio;
    }
    if height < c.min_height {
        height = c.min_height;
        width = height * ratio;
    }
    let size = c.constrain(Size::flat(width, height));
    Some((size.width, size.height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{LayoutProbe, Log, layout_of, rc};
    use crate::widgets::{Center, SizedBox};

    const W: Size = Size::flat(100.0, 100.0);

    #[test]
    fn aspect_ratio_uses_max_width() {
        assert_eq!(
            layout_of(
                W,
                rc(Center::new(rc(AspectRatio::new(2.0, rc(SizedBox::new())))))
            ),
            "Center 100x100x0 @ 0,0,0\n  AspectRatio 100x50x0 @ 0,25,0\n    SizedBox 100x50x0 @ 0,0,0\n"
        );
    }

    #[test]
    fn aspect_ratio_limited_by_height() {
        assert!(
            layout_of(
                W,
                rc(Center::new(rc(AspectRatio::new(0.5, rc(SizedBox::new())))))
            )
            .contains("\n  AspectRatio 50x100x0 @ 25,0,0\n")
        );
    }

    fn unbounded_aspect_ratio() -> String {
        let log = Log::default();
        layout_of(
            W,
            rc(LayoutProbe::new("root", &log)
                .child_constraints(Constraints::unbounded())
                .children(vec![rc(AspectRatio::new(2.0, rc(SizedBox::new())))])),
        )
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "AspectRatio precisa de largura ou altura limitada")]
    fn aspect_ratio_without_bounds_panics_in_debug() {
        unbounded_aspect_ratio();
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn aspect_ratio_without_bounds_takes_smallest_in_release() {
        assert!(unbounded_aspect_ratio().contains("\n  AspectRatio 0x0x0 @ 0,0,0\n"));
    }
}
