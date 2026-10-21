use std::rc::Rc;

use crate::context::BuildContext;
use crate::element::{Element, ElementCore};
use crate::geometry::{Axis, Constraints, Offset, Size};
use crate::layout::impossible_layout;
use crate::widget::Widget;

/// How tall each grid row is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridRowHeight {
    /// Every cell is `width / ratio` tall.
    AspectRatio(f32),
    /// Each row is as tall as its tallest child; children sit at the top.
    Content,
}

/// Lays its children out in a fixed number of equal-width columns, filling
/// row by row. Needs a bounded width.
pub struct Grid {
    columns: usize,
    row_spacing: f32,
    column_spacing: f32,
    row_height: GridRowHeight,
    children: Vec<Rc<dyn Widget>>,
}

impl Grid {
    pub fn new(columns: usize, children: Vec<Rc<dyn Widget>>) -> Grid {
        Grid {
            columns,
            row_spacing: 0.0,
            column_spacing: 0.0,
            row_height: GridRowHeight::AspectRatio(1.0),
            children,
        }
    }

    pub fn row_spacing(mut self, spacing: f32) -> Grid {
        self.row_spacing = spacing;
        self
    }

    pub fn column_spacing(mut self, spacing: f32) -> Grid {
        self.column_spacing = spacing;
        self
    }

    pub fn row_height(mut self, height: GridRowHeight) -> Grid {
        self.row_height = height;
        self
    }
}

impl Widget for Grid {
    fn create_element(self: Rc<Self>) -> Box<dyn Element> {
        Box::new(GridElement {
            core: ElementCore::new(self),
        })
    }
}

struct GridElement {
    core: ElementCore,
}

impl Element for GridElement {
    fn core(&self) -> &ElementCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ElementCore {
        &mut self.core
    }

    fn build(&mut self, _ctx: &mut BuildContext<'_>) -> Vec<Rc<dyn Widget>> {
        self.core.widget_as::<Grid>().children.clone()
    }

    fn layout(&mut self, constraints: Constraints) -> Size {
        let grid = self.core.widget_as::<Grid>();
        grid_layout(&mut self.core, &grid, constraints)
    }
}

fn grid_layout(core: &mut ElementCore, grid: &Grid, c: Constraints) -> Size {
    let count = core.children().len();
    let message = if grid.columns == 0 {
        Some("Grid precisa de pelo menos uma coluna")
    } else if !c.has_bounded(Axis::X) {
        Some("Grid precisa de largura limitada")
    } else {
        None
    };
    if let Some(message) = message {
        impossible_layout(message);
        for index in 0..count {
            core.layout_child(index, Constraints::tight(Size::ZERO), true);
            core.set_child_offset(index, Offset::ZERO);
        }
        return c.smallest();
    }

    let columns = grid.columns as f32;
    let cell_width = ((c.max_width - grid.column_spacing * (columns - 1.0)) / columns).max(0.0);
    let child_constraints = match grid.row_height {
        GridRowHeight::AspectRatio(ratio) => {
            Constraints::tight(Size::flat(cell_width, cell_width / ratio))
        }
        GridRowHeight::Content => {
            Constraints::unbounded().with_axis(Axis::X, cell_width, cell_width)
        }
    }
    .with_axis(Axis::Z, 0.0, c.max_depth);
    let sizes: Vec<Size> = (0..count)
        .map(|index| core.layout_child(index, child_constraints, true))
        .collect();

    let rows = count.div_ceil(grid.columns);
    let mut y = 0.0;
    for row in 0..rows {
        let cells = row * grid.columns..((row + 1) * grid.columns).min(count);
        let row_height = match grid.row_height {
            GridRowHeight::AspectRatio(ratio) => cell_width / ratio,
            GridRowHeight::Content => sizes[cells.clone()]
                .iter()
                .map(|size| size.height)
                .fold(0.0, f32::max),
        };
        for (column, index) in cells.enumerate() {
            let x = column as f32 * (cell_width + grid.column_spacing);
            core.set_child_offset(index, Offset::new(x, y, 0.0));
        }
        y += row_height;
        if row + 1 < rows {
            y += grid.row_spacing;
        }
    }
    let depth = sizes.iter().map(|size| size.depth).fold(0.0, f32::max);
    c.constrain(Size::new(c.max_width, y, depth))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{app_with, layout_of, rc, root_children, unbounded_layout};
    use crate::widgets::{Center, SizedBox};

    const W: Size = Size::flat(210.0, 300.0);

    fn cells() -> Vec<Rc<dyn Widget>> {
        vec![
            rc(SizedBox::new()),
            rc(SizedBox::new()),
            rc(SizedBox::new()),
        ]
    }

    fn grid_cells(grid: Grid) -> Vec<(Size, Offset)> {
        let app = app_with(W, rc(Center::new(rc(grid))));
        root_children(&app)[0]
            .children()
            .iter()
            .map(|c| (c.size(), c.offset()))
            .collect()
    }

    #[test]
    fn grid_square_cells() {
        let grid = Grid::new(2, cells()).column_spacing(10.0).row_spacing(10.0);
        assert!(layout_of(W, rc(Center::new(rc(grid)))).contains(
            "\n  Grid 210x210x0 @ 0,45,0\n    SizedBox 100x100x0 @ 0,0,0\n    SizedBox 100x100x0 @ 110,0,0\n    SizedBox 100x100x0 @ 0,110,0\n"
        ));
    }

    #[test]
    fn grid_aspect_ratio_cells() {
        let grid = Grid::new(2, cells())
            .column_spacing(10.0)
            .row_spacing(10.0)
            .row_height(GridRowHeight::AspectRatio(2.0));
        let cells = grid_cells(grid);
        assert!(
            cells
                .iter()
                .all(|(size, _)| *size == Size::flat(100.0, 50.0))
        );
        assert_eq!(cells[2].1, Offset::new(0.0, 60.0, 0.0));
    }

    #[test]
    fn grid_content_row_height() {
        let grid = Grid::new(
            2,
            vec![
                rc(SizedBox::new().height(30.0)),
                rc(SizedBox::new().height(50.0)),
                rc(SizedBox::new().height(20.0)),
            ],
        )
        .column_spacing(10.0)
        .row_spacing(10.0)
        .row_height(GridRowHeight::Content);
        assert_eq!(
            grid_cells(grid),
            [
                (Size::flat(100.0, 30.0), Offset::new(0.0, 0.0, 0.0)),
                (Size::flat(100.0, 50.0), Offset::new(110.0, 0.0, 0.0)),
                (Size::flat(100.0, 20.0), Offset::new(0.0, 60.0, 0.0)),
            ]
        );
    }

    #[test]
    fn grid_without_children_is_flat() {
        assert!(
            layout_of(W, rc(Center::new(rc(Grid::new(3, vec![])))))
                .contains("\n  Grid 210x0x0 @ 0,150,0\n")
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Grid precisa de largura limitada")]
    fn grid_needs_bounded_width() {
        unbounded_layout(W, rc(Grid::new(2, cells())));
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "Grid precisa de pelo menos uma coluna")]
    fn grid_needs_columns() {
        layout_of(W, rc(Center::new(rc(Grid::new(0, cells())))));
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn impossible_grid_is_smallest_with_zero_children() {
        let unbounded = unbounded_layout(W, rc(Grid::new(2, cells())));
        assert!(unbounded.contains("\n  Grid 0x0x0 @ 0,0,0\n    SizedBox 0x0x0 @ 0,0,0\n"));
        let no_columns = layout_of(W, rc(Center::new(rc(Grid::new(0, cells())))));
        assert!(no_columns.contains("\n  Grid 0x0x0 @ 105,150,0\n    SizedBox 0x0x0 @ 0,0,0\n"));
    }
}
