//! Custom layouts written the way another library would: only through the
//! public API.

use std::rc::Rc;

use notify_rs::{
    App, BuildContext, Center, Constraints, Element, ElementCore, Expanded, FlexParentData, Offset,
    Size, SizedBox, Spacer, Widget,
};

/// Places each child at the end of the previous one, diagonally.
struct Diagonal {
    children: Vec<Rc<dyn Widget>>,
}

impl Widget for Diagonal {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(DiagonalElement {
            core: ElementCore::new(self),
        })
    }
}

struct DiagonalElement {
    core: ElementCore,
}

impl Element for DiagonalElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<Diagonal>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let mut position = Offset::ZERO;
        let mut total = Size::ZERO;
        for index in 0..self.core.children().len() {
            let size = self.core.layout_child(index, constraints.loosen(), true);
            self.core.set_child_offset(index, position);
            position = Offset::new(position.x + size.width, position.y + size.height, 0.0);
            total = Size::new(
                total.width + size.width,
                total.height + size.height,
                total.depth.max(size.depth),
            );
        }
        constraints.constrain(total)
    }
}

/// As wide as the sum of its children's flex factors.
struct FlexSum {
    children: Vec<Rc<dyn Widget>>,
}

impl Widget for FlexSum {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(FlexSumElement {
            core: ElementCore::new(self),
        })
    }
}

struct FlexSumElement {
    core: ElementCore,
}

impl Element for FlexSumElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<FlexSum>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let mut width = 0.0;
        for index in 0..self.core.children().len() {
            self.core.layout_child(index, constraints.loosen(), true);
            let flex = self.core.children()[index]
                .parent_data()
                .and_then(|data| data.downcast_ref::<FlexParentData>())
                .map_or(0, |data| data.flex);
            width += flex as f32;
        }
        constraints.constrain(Size::flat(width, 0.0))
    }
}

#[test]
fn custom_layout_element_positions_children() {
    let mut app = App::new();
    app.set_size(Size::flat(100.0, 100.0));
    app.set_root(Rc::new(Diagonal {
        children: vec![
            Rc::new(SizedBox::new().width(10.0).height(20.0)),
            Rc::new(SizedBox::new().width(30.0).height(5.0)),
        ],
    }));
    let children = app.debug_layout();
    assert_eq!(
        children,
        "Diagonal 100x100x0 @ 0,0,0\n  SizedBox 10x20x0 @ 0,0,0\n  SizedBox 30x5x0 @ 10,20,0\n"
    );
}

#[test]
fn custom_element_reads_flex_parent_data() {
    let mut app = App::new();
    app.set_size(Size::flat(100.0, 100.0));
    app.set_root(Rc::new(Center::new(Rc::new(FlexSum {
        children: vec![
            Rc::new(Expanded::new(Rc::new(SizedBox::new())).flex(2)),
            Rc::new(Spacer::new().flex(3)),
        ],
    }))));
    assert!(app.debug_layout().contains("\n  FlexSum 5x0x0 @ "));
}
