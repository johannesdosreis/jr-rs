use std::any::Any;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Axis, Constraints, Offset, Size};
use crate::layout::impossible_layout;
use crate::widget::Widget;

/// How children are distributed along the main axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainAxisAlignment {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How children are placed on both cross axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    End,
    Center,
    Stretch,
}

/// Whether a flex takes all the main axis space it is allowed, or only what its
/// children use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MainAxisSize {
    Max,
    Min,
}

/// Whether a flexible child must fill its share (`Tight`) or may be smaller
/// (`Loose`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlexFit {
    Tight,
    Loose,
}

/// The parent data of `Expanded`, `Flexible` and `Spacer`, read by flex layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlexParentData {
    pub flex: u32,
    pub fit: FlexFit,
}

#[derive(Clone, Copy, Debug)]
struct FlexConfig {
    axis: Axis,
    main_alignment: MainAxisAlignment,
    cross_alignment: CrossAxisAlignment,
    main_size: MainAxisSize,
    spacing: f32,
}

impl FlexConfig {
    fn new(axis: Axis) -> FlexConfig {
        FlexConfig {
            axis,
            main_alignment: MainAxisAlignment::Start,
            cross_alignment: CrossAxisAlignment::Center,
            main_size: MainAxisSize::Max,
            spacing: 0.0,
        }
    }
}

trait FlexWidget: Widget {
    fn config(&self) -> &FlexConfig;
    fn flex_children(&self) -> &[Rc<dyn Widget>];
}

macro_rules! flex_widget {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        pub struct $name {
            config: FlexConfig,
            children: Vec<Rc<dyn Widget>>,
        }

        impl $name {
            pub fn main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
                self.config.main_alignment = alignment;
                self
            }

            pub fn cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
                self.config.cross_alignment = alignment;
                self
            }

            pub fn main_axis_size(mut self, size: MainAxisSize) -> Self {
                self.config.main_size = size;
                self
            }

            /// Space between consecutive children.
            pub fn spacing(mut self, spacing: f32) -> Self {
                self.config.spacing = spacing;
                self
            }
        }

        impl FlexWidget for $name {
            fn config(&self) -> &FlexConfig {
                &self.config
            }

            fn flex_children(&self) -> &[Rc<dyn Widget>] {
                &self.children
            }
        }

        impl Widget for $name {
            fn create_element(self: Rc<Self>) -> Box<dyn Element> {
                Box::new(FlexElement::<$name> {
                    core: ElementCore::new(self),
                    _marker: PhantomData,
                })
            }
        }
    };
}

flex_widget!(
    /// Lays its children out along `axis`; `Axis::Z` stacks them back to front.
    Flex
);
flex_widget!(
    /// A [`Flex`] along `Axis::X`.
    Row
);
flex_widget!(
    /// A [`Flex`] along `Axis::Y`.
    Column
);

impl Flex {
    pub fn new(axis: Axis, children: Vec<Rc<dyn Widget>>) -> Flex {
        Flex {
            config: FlexConfig::new(axis),
            children,
        }
    }
}

impl Row {
    pub fn new(children: Vec<Rc<dyn Widget>>) -> Row {
        Row {
            config: FlexConfig::new(Axis::X),
            children,
        }
    }
}

impl Column {
    pub fn new(children: Vec<Rc<dyn Widget>>) -> Column {
        Column {
            config: FlexConfig::new(Axis::Y),
            children,
        }
    }
}

struct FlexElement<W> {
    core: ElementCore,
    _marker: PhantomData<fn() -> W>,
}

impl<W: FlexWidget> Element for FlexElement<W> {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<W>().flex_children().to_vec()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let config = *self.core.widget_as::<W>().config();
        flex_layout(&mut self.core, &config, constraints)
    }
}

fn cross_axes(main: Axis) -> [Axis; 2] {
    match main {
        Axis::X => [Axis::Y, Axis::Z],
        Axis::Y => [Axis::X, Axis::Z],
        Axis::Z => [Axis::X, Axis::Y],
    }
}

fn flex_layout(core: &mut ElementCore, config: &FlexConfig, c: Constraints) -> Size {
    let main = config.axis;
    let cross = cross_axes(main);
    let count = core.children().len();

    let mut stretch = [false; 2];
    if config.cross_alignment == CrossAxisAlignment::Stretch {
        for (index, &axis) in cross.iter().enumerate() {
            if c.has_bounded(axis) {
                stretch[index] = true;
            } else {
                impossible_layout("Flex com Stretch precisa de eixos transversais limitados");
            }
        }
    }
    let base =
        cross
            .iter()
            .zip(stretch)
            .fold(Constraints::unbounded(), |base, (&axis, stretched)| {
                let min = if stretched { c.max(axis) } else { 0.0 };
                base.with_axis(axis, min, c.max(axis))
            });
    let spacing_total = config.spacing * count.saturating_sub(1) as f32;

    let flex_data: Vec<Option<FlexParentData>> = core
        .children()
        .iter()
        .map(|child| {
            child
                .parent_data()
                .and_then(|data| data.downcast_ref::<FlexParentData>())
                .filter(|data| data.flex > 0)
                .copied()
        })
        .collect();

    let mut sizes = vec![Size::ZERO; count];
    let mut non_flex_main = 0.0;
    for (index, data) in flex_data.iter().enumerate() {
        if data.is_none() {
            sizes[index] = core.layout_child(index, base, true);
            non_flex_main += sizes[index].along(main);
        }
    }

    let total_flex: u32 = flex_data.iter().flatten().map(|data| data.flex).sum();
    if total_flex > 0 {
        let free = if c.has_bounded(main) {
            (c.max(main) - non_flex_main - spacing_total).max(0.0)
        } else {
            impossible_layout("Flex com filhos flexíveis precisa de eixo principal limitado");
            0.0
        };
        for (index, data) in flex_data.iter().enumerate() {
            let Some(data) = data else { continue };
            let allotted = free * data.flex as f32 / total_flex as f32;
            let min = match data.fit {
                FlexFit::Tight => allotted,
                FlexFit::Loose => 0.0,
            };
            sizes[index] = core.layout_child(index, base.with_axis(main, min, allotted), true);
        }
    }

    let used = sizes.iter().map(|size| size.along(main)).sum::<f32>() + spacing_total;
    let main_extent = if config.main_size == MainAxisSize::Max && c.has_bounded(main) {
        c.max(main)
    } else {
        used
    };
    let mut size = Size::ZERO.with_axis(main, main_extent);
    for (index, &axis) in cross.iter().enumerate() {
        let extent = if stretch[index] {
            c.max(axis)
        } else {
            sizes
                .iter()
                .map(|size| size.along(axis))
                .fold(0.0, f32::max)
        };
        size = size.with_axis(axis, extent);
    }
    let size = c.constrain(size);

    let (leading, between) = distribute(
        config.main_alignment,
        size.along(main) - used,
        count,
        config.spacing,
    );
    let mut position = leading;
    for (index, child) in sizes.iter().enumerate() {
        let mut offset = Offset::ZERO.with_axis(main, position);
        for &axis in &cross {
            let room = size.along(axis) - child.along(axis);
            let value = match config.cross_alignment {
                CrossAxisAlignment::Start | CrossAxisAlignment::Stretch => 0.0,
                CrossAxisAlignment::End => room,
                CrossAxisAlignment::Center => room / 2.0,
            };
            offset = offset.with_axis(axis, value);
        }
        core.set_child_offset(index, offset);
        position += child.along(main) + between;
    }
    size
}

/// The leading space and the space between `count` children that distribute
/// `free` space by `alignment`. Without free space (overflow), children start
/// at the beginning, separated by `spacing` only.
pub(crate) fn distribute(
    alignment: MainAxisAlignment,
    free: f32,
    count: usize,
    spacing: f32,
) -> (f32, f32) {
    if free < 0.0 || count == 0 {
        return (0.0, spacing);
    }
    let count_f = count as f32;
    match alignment {
        MainAxisAlignment::Start => (0.0, spacing),
        MainAxisAlignment::End => (free, spacing),
        MainAxisAlignment::Center => (free / 2.0, spacing),
        MainAxisAlignment::SpaceBetween if count > 1 => (0.0, spacing + free / (count_f - 1.0)),
        MainAxisAlignment::SpaceBetween => (0.0, spacing),
        MainAxisAlignment::SpaceAround => (free / count_f / 2.0, spacing + free / count_f),
        MainAxisAlignment::SpaceEvenly => {
            let gap = free / (count_f + 1.0);
            (gap, spacing + gap)
        }
    }
}

/// A widget that a flex layout sizes by a flex factor. Outside a flex it has
/// no effect and its child passes through.
trait FlexChild: Widget {
    fn data(&self) -> &FlexParentData;
    fn child_widget(&self) -> Option<&Rc<dyn Widget>>;
}

/// Makes its child fill a share of the flex's free space proportional to `flex`.
pub struct Expanded {
    data: FlexParentData,
    child: Rc<dyn Widget>,
}

/// Lets its child take up to a share of the flex's free space proportional to `flex`.
pub struct Flexible {
    data: FlexParentData,
    child: Rc<dyn Widget>,
}

/// Empty flexible space inside a flex.
pub struct Spacer {
    data: FlexParentData,
}

impl Expanded {
    pub fn new(child: Rc<dyn Widget>) -> Expanded {
        Expanded {
            data: FlexParentData {
                flex: 1,
                fit: FlexFit::Tight,
            },
            child,
        }
    }

    pub fn flex(mut self, flex: u32) -> Expanded {
        self.data.flex = flex;
        self
    }
}

impl Flexible {
    pub fn new(child: Rc<dyn Widget>) -> Flexible {
        Flexible {
            data: FlexParentData {
                flex: 1,
                fit: FlexFit::Loose,
            },
            child,
        }
    }

    pub fn flex(mut self, flex: u32) -> Flexible {
        self.data.flex = flex;
        self
    }
}

impl Spacer {
    pub fn new() -> Spacer {
        Spacer {
            data: FlexParentData {
                flex: 1,
                fit: FlexFit::Tight,
            },
        }
    }

    pub fn flex(mut self, flex: u32) -> Spacer {
        self.data.flex = flex;
        self
    }
}

impl Default for Spacer {
    fn default() -> Spacer {
        Spacer::new()
    }
}

impl FlexChild for Expanded {
    fn data(&self) -> &FlexParentData {
        &self.data
    }

    fn child_widget(&self) -> Option<&Rc<dyn Widget>> {
        Some(&self.child)
    }
}

impl FlexChild for Flexible {
    fn data(&self) -> &FlexParentData {
        &self.data
    }

    fn child_widget(&self) -> Option<&Rc<dyn Widget>> {
        Some(&self.child)
    }
}

impl FlexChild for Spacer {
    fn data(&self) -> &FlexParentData {
        &self.data
    }

    fn child_widget(&self) -> Option<&Rc<dyn Widget>> {
        None
    }
}

macro_rules! flex_child_widget {
    ($name:ident) => {
        impl Widget for $name {
            fn create_element(self: Rc<Self>) -> Box<dyn Element> {
                Box::new(FlexChildElement::<$name> {
                    core: ElementCore::new(self),
                    _marker: PhantomData,
                })
            }
        }
    };
}

flex_child_widget!(Expanded);
flex_child_widget!(Flexible);
flex_child_widget!(Spacer);

struct FlexChildElement<W> {
    core: ElementCore,
    _marker: PhantomData<fn() -> W>,
}

impl<W: FlexChild> Element for FlexChildElement<W> {
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

    fn parent_data(&self) -> Option<&dyn Any> {
        let widget: &dyn Any = &**self.core.widget();
        widget
            .downcast_ref::<W>()
            .map(|widget| widget.data() as &dyn Any)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::geometry::Size;
    use crate::test_support::{
        LayoutProbe, Log, Probe, app_with, layout_of, rc, root_children, sized, unbounded_layout,
    };
    use crate::widgets::{Center, SizedBox};

    const W: Size = Size::flat(300.0, 100.0);

    fn xs(app: &App) -> Vec<f32> {
        root_children(app).iter().map(|c| c.offset().x).collect()
    }

    fn widths(app: &App) -> Vec<f32> {
        root_children(app).iter().map(|c| c.size().width).collect()
    }

    fn three() -> Vec<Rc<dyn Widget>> {
        vec![sized(50.0, 20.0), sized(50.0, 20.0), sized(50.0, 20.0)]
    }

    fn unbounded(child: Rc<dyn Widget>) -> String {
        unbounded_layout(W, child)
    }

    #[test]
    fn row_places_children_from_start() {
        assert_eq!(
            layout_of(W, rc(Row::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)]))),
            "Row 300x100x0 @ 0,0,0\n  SizedBox 50x20x0 @ 0,40,0\n  SizedBox 30x40x0 @ 50,30,0\n"
        );
    }

    #[test]
    fn main_axis_alignments() {
        use MainAxisAlignment::*;
        let cases = [
            (Start, [0.0, 50.0, 100.0]),
            (End, [150.0, 200.0, 250.0]),
            (Center, [75.0, 125.0, 175.0]),
            (SpaceBetween, [0.0, 125.0, 250.0]),
            (SpaceAround, [25.0, 125.0, 225.0]),
            (SpaceEvenly, [37.5, 125.0, 212.5]),
        ];
        for (alignment, expected) in cases {
            let app = app_with(W, rc(Row::new(three()).main_axis_alignment(alignment)));
            assert_eq!(xs(&app), expected, "{alignment:?}");
        }
    }

    #[test]
    fn cross_axis_alignments() {
        use CrossAxisAlignment::*;
        for (alignment, y, height) in [
            (Start, 0.0, 20.0),
            (End, 80.0, 20.0),
            (Center, 40.0, 20.0),
            (Stretch, 0.0, 100.0),
        ] {
            let app = app_with(
                W,
                rc(Row::new(vec![sized(50.0, 20.0)]).cross_axis_alignment(alignment)),
            );
            let child = &root_children(&app)[0];
            assert_eq!(
                (child.offset().y, child.size().height),
                (y, height),
                "{alignment:?}"
            );
        }
    }

    #[test]
    fn main_axis_size_min_shrinks() {
        let row =
            Row::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)]).main_axis_size(MainAxisSize::Min);
        assert!(layout_of(W, rc(Center::new(rc(row)))).contains("\n  Row 80x40x0 @ 110,30,0\n"));
    }

    #[test]
    fn spacing_adds_gaps() {
        let app = app_with(W, rc(Row::new(three()).spacing(10.0)));
        assert_eq!(xs(&app), [0.0, 60.0, 120.0]);
    }

    #[test]
    fn expanded_takes_remaining_space() {
        let app = app_with(
            W,
            rc(Row::new(vec![
                sized(50.0, 20.0),
                rc(Expanded::new(sized(10.0, 20.0))),
                sized(50.0, 20.0),
            ])),
        );
        assert_eq!(xs(&app), [0.0, 50.0, 250.0]);
        assert!(
            app.debug_layout()
                .contains("\n  Expanded 200x20x0 @ 50,40,0\n    SizedBox 200x20x0 @ 0,0,0\n")
        );
    }

    #[test]
    fn flex_factors_split_space() {
        let app = app_with(
            W,
            rc(Row::new(vec![
                rc(Expanded::new(rc(SizedBox::new()))),
                rc(Expanded::new(rc(SizedBox::new())).flex(2)),
            ])),
        );
        assert_eq!(widths(&app), [100.0, 200.0]);
    }

    #[test]
    fn flexible_is_loose() {
        let app = app_with(
            W,
            rc(Row::new(vec![
                rc(Flexible::new(sized(50.0, 20.0))),
                sized(50.0, 20.0),
            ])),
        );
        assert_eq!(xs(&app), [0.0, 50.0]);
    }

    #[test]
    fn spacer_pushes_children_apart() {
        let app = app_with(
            W,
            rc(Row::new(vec![
                sized(50.0, 20.0),
                rc(Spacer::new()),
                sized(50.0, 20.0),
            ])),
        );
        assert_eq!(xs(&app), [0.0, 50.0, 250.0]);
        assert!(
            app.debug_layout()
                .contains("\n  Spacer 200x0x0 @ 50,50,0\n")
        );
    }

    #[test]
    fn children_overflow_from_start() {
        let app = app_with(
            W,
            rc(Row::new(vec![sized(200.0, 20.0), sized(200.0, 20.0)])
                .main_axis_alignment(MainAxisAlignment::Center)),
        );
        assert_eq!(xs(&app), [0.0, 200.0]);
        assert_eq!(app.root.as_ref().unwrap().size().width, 300.0);
    }

    #[test]
    fn column_lays_out_vertically() {
        assert_eq!(
            layout_of(
                W,
                rc(Column::new(vec![sized(50.0, 20.0), sized(30.0, 40.0)]))
            ),
            "Column 300x100x0 @ 0,0,0\n  SizedBox 50x20x0 @ 125,0,0\n  SizedBox 30x40x0 @ 135,20,0\n"
        );
    }

    #[test]
    fn flex_along_z_stacks_back_to_front() {
        let cube = |depth| rc(SizedBox::new().width(10.0).height(10.0).depth(depth));
        assert_eq!(
            layout_of(
                Size::new(100.0, 100.0, 90.0),
                rc(Flex::new(Axis::Z, vec![cube(10.0), cube(20.0)]))
            ),
            "Flex 100x100x90 @ 0,0,0\n  SizedBox 10x10x10 @ 45,45,0\n  SizedBox 10x10x20 @ 45,45,10\n"
        );
    }

    #[test]
    fn center_inside_row_takes_child_width() {
        assert!(
            layout_of(W, rc(Row::new(vec![rc(Center::new(sized(10.0, 10.0)))])))
                .contains("\n  Center 10x100x0 @ 0,0,0\n    SizedBox 10x10x0 @ 0,45,0\n")
        );
    }

    #[test]
    fn expanded_outside_flex_passes_through() {
        assert!(
            layout_of(
                Size::flat(100.0, 100.0),
                rc(Center::new(rc(Expanded::new(sized(10.0, 10.0)))))
            )
            .contains("\n  Expanded 10x10x0 @ 45,45,0\n")
        );
    }

    #[test]
    fn changing_flex_factor_changes_sizes() {
        let log = Log::default();
        let row = |flex| {
            rc(Row::new(vec![
                rc(Expanded::new(rc(LayoutProbe::new("a", &log))).flex(flex)),
                rc(Expanded::new(rc(LayoutProbe::new("b", &log)))),
            ]))
        };
        let mut app = app_with(W, row(1));
        assert_eq!(widths(&app), [150.0, 150.0]);
        app.set_root(row(2));
        assert_eq!(widths(&app), [200.0, 100.0]);
    }

    fn item(log: &Log, name: &'static str, key: i32, width: f32) -> Rc<dyn Widget> {
        rc(Probe::new(name, log)
            .with_key(key)
            .with_children(vec![sized(width, 20.0)]))
    }

    #[test]
    fn inserted_child_is_laid_out_in_the_same_update() {
        let log = Log::default();
        let mut app = app_with(
            W,
            rc(Row::new(vec![
                item(&log, "a", 1, 50.0),
                item(&log, "b", 2, 50.0),
            ])),
        );
        app.set_root(rc(Row::new(vec![
            item(&log, "a", 1, 50.0),
            item(&log, "c", 3, 30.0),
            item(&log, "b", 2, 50.0),
        ])));
        assert_eq!(xs(&app), [0.0, 50.0, 80.0]);
        assert_eq!(root_children(&app)[1].size(), Size::flat(30.0, 20.0));
    }

    #[test]
    fn reordered_keyed_children_get_new_offsets() {
        let log = Log::default();
        let mut app = app_with(
            W,
            rc(Row::new(vec![
                item(&log, "a", 1, 50.0),
                item(&log, "b", 2, 30.0),
            ])),
        );
        log.take();
        app.set_root(rc(Row::new(vec![
            item(&log, "b", 2, 30.0),
            item(&log, "a", 1, 50.0),
        ])));
        assert_eq!(xs(&app), [0.0, 30.0]);
        assert!(!log.take().iter().any(|entry| entry.starts_with("create")));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Flex com filhos flexíveis precisa de eixo principal limitado")]
    fn flex_children_need_bounded_main_axis() {
        unbounded(rc(Row::new(vec![rc(Expanded::new(sized(10.0, 10.0)))])));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn flex_children_get_zero_without_bounded_main_axis() {
        assert!(
            unbounded(rc(Row::new(vec![rc(Expanded::new(sized(10.0, 10.0)))])))
                .contains("\n    Expanded 0x10x0 @ 0,0,0\n")
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Flex com Stretch precisa de eixos transversais limitados")]
    fn stretch_needs_bounded_cross_axes() {
        unbounded(rc(
            Row::new(vec![sized(10.0, 10.0)]).cross_axis_alignment(CrossAxisAlignment::Stretch)
        ));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn stretch_falls_back_to_start_without_bounds() {
        assert!(
            unbounded(rc(
                Row::new(vec![sized(10.0, 10.0)]).cross_axis_alignment(CrossAxisAlignment::Stretch)
            ))
            .contains("\n    SizedBox 10x10x0 @ 0,0,0\n")
        );
    }
}
