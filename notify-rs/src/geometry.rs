//! 3D layout geometry. x grows to the right, y downwards and z towards the
//! viewer; an offset is the position of an element's minimum corner (left,
//! top, back) relative to its parent's.

/// One of the three layout axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
}

/// Width, height and depth in logical pixels. A flat widget has depth 0.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
    pub depth: f32,
}

impl Size {
    pub const ZERO: Size = Size::new(0.0, 0.0, 0.0);

    pub const fn new(width: f32, height: f32, depth: f32) -> Size {
        Size {
            width,
            height,
            depth,
        }
    }

    pub const fn flat(width: f32, height: f32) -> Size {
        Size::new(width, height, 0.0)
    }

    pub fn along(self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.width,
            Axis::Y => self.height,
            Axis::Z => self.depth,
        }
    }

    pub fn with_axis(mut self, axis: Axis, value: f32) -> Size {
        match axis {
            Axis::X => self.width = value,
            Axis::Y => self.height = value,
            Axis::Z => self.depth = value,
        }
        self
    }
}

/// A position in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Offset {
    pub const ZERO: Offset = Offset::new(0.0, 0.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Offset {
        Offset { x, y, z }
    }

    pub fn along(self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    pub fn with_axis(mut self, axis: Axis, value: f32) -> Offset {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
        self
    }
}

/// The range of sizes a parent allows a child, per axis. Maximums may be
/// infinite; minimums are finite.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub min_depth: f32,
    pub max_depth: f32,
}

impl Constraints {
    pub fn tight(size: Size) -> Constraints {
        Constraints {
            min_width: size.width,
            max_width: size.width,
            min_height: size.height,
            max_height: size.height,
            min_depth: size.depth,
            max_depth: size.depth,
        }
    }

    pub fn loose(size: Size) -> Constraints {
        Constraints::tight(size).loosen()
    }

    /// Tight and infinite on every axis: only meaningful as the argument of `enforce`.
    pub fn expand() -> Constraints {
        Constraints::tight(Size::new(f32::INFINITY, f32::INFINITY, f32::INFINITY))
    }

    pub fn unbounded() -> Constraints {
        Constraints::loose(Size::new(f32::INFINITY, f32::INFINITY, f32::INFINITY))
    }

    pub fn min(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.min_width,
            Axis::Y => self.min_height,
            Axis::Z => self.min_depth,
        }
    }

    pub fn max(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.max_width,
            Axis::Y => self.max_height,
            Axis::Z => self.max_depth,
        }
    }

    pub fn has_bounded(&self, axis: Axis) -> bool {
        self.max(axis).is_finite()
    }

    pub fn with_axis(mut self, axis: Axis, min: f32, max: f32) -> Constraints {
        match axis {
            Axis::X => (self.min_width, self.max_width) = (min, max),
            Axis::Y => (self.min_height, self.max_height) = (min, max),
            Axis::Z => (self.min_depth, self.max_depth) = (min, max),
        }
        self
    }

    /// Makes `axis` exact at `value`, kept inside the axis' current range.
    pub fn tighten(self, axis: Axis, value: f32) -> Constraints {
        let value = clamp(value, self.min(axis), self.max(axis));
        self.with_axis(axis, value, value)
    }

    pub fn is_tight(&self) -> bool {
        Axis::ALL
            .iter()
            .all(|&axis| self.min(axis) == self.max(axis))
    }

    pub fn constrain(&self, size: Size) -> Size {
        Axis::ALL.iter().fold(size, |size, &axis| {
            let value = clamp(size.along(axis), self.min(axis), self.max(axis));
            size.with_axis(axis, value)
        })
    }

    pub fn loosen(mut self) -> Constraints {
        self.min_width = 0.0;
        self.min_height = 0.0;
        self.min_depth = 0.0;
        self
    }

    /// `self` clamped into `other`'s range on every axis: the result always fits `other`.
    pub fn enforce(self, other: Constraints) -> Constraints {
        Axis::ALL.iter().fold(self, |result, &axis| {
            let (low, high) = (other.min(axis), other.max(axis));
            result.with_axis(
                axis,
                clamp(self.min(axis), low, high),
                clamp(self.max(axis), low, high),
            )
        })
    }

    pub fn deflate(self, insets: EdgeInsets) -> Constraints {
        let totals = [insets.horizontal(), insets.vertical(), insets.depth()];
        Axis::ALL
            .iter()
            .zip(totals)
            .fold(self, |result, (&axis, total)| {
                let min = (self.min(axis) - total).max(0.0);
                let max = (self.max(axis) - total).max(min);
                result.with_axis(axis, min, max)
            })
    }

    pub fn biggest(&self) -> Size {
        self.constrain(Size::new(f32::INFINITY, f32::INFINITY, f32::INFINITY))
    }

    pub fn smallest(&self) -> Size {
        Size::new(self.min_width, self.min_height, self.min_depth)
    }
}

/// Like `f32::clamp`, but never panics: invalid bounds (min above max, NaN)
/// give a meaningless value instead, which layout validation then reports.
fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

/// Space around a child: four sides in the plane plus front and back in depth.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct EdgeInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub front: f32,
    pub back: f32,
}

impl EdgeInsets {
    pub const ZERO: EdgeInsets = EdgeInsets {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        front: 0.0,
        back: 0.0,
    };

    /// The same value on the four sides of the plane; no depth.
    pub fn all(value: f32) -> EdgeInsets {
        EdgeInsets::symmetric(value, value)
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> EdgeInsets {
        EdgeInsets {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
            ..EdgeInsets::ZERO
        }
    }

    pub fn left(mut self, value: f32) -> EdgeInsets {
        self.left = value;
        self
    }

    pub fn top(mut self, value: f32) -> EdgeInsets {
        self.top = value;
        self
    }

    pub fn right(mut self, value: f32) -> EdgeInsets {
        self.right = value;
        self
    }

    pub fn bottom(mut self, value: f32) -> EdgeInsets {
        self.bottom = value;
        self
    }

    pub fn with_depth(mut self, front: f32, back: f32) -> EdgeInsets {
        self.front = front;
        self.back = back;
        self
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }

    pub fn depth(&self) -> f32 {
        self.front + self.back
    }
}

/// Where a child sits inside its parent, from -1 (start) to 1 (end) on each
/// axis. The plane constants are centered in depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Alignment {
    pub const TOP_LEFT: Alignment = Alignment::new(-1.0, -1.0, 0.0);
    pub const TOP_CENTER: Alignment = Alignment::new(0.0, -1.0, 0.0);
    pub const TOP_RIGHT: Alignment = Alignment::new(1.0, -1.0, 0.0);
    pub const CENTER_LEFT: Alignment = Alignment::new(-1.0, 0.0, 0.0);
    pub const CENTER: Alignment = Alignment::new(0.0, 0.0, 0.0);
    pub const CENTER_RIGHT: Alignment = Alignment::new(1.0, 0.0, 0.0);
    pub const BOTTOM_LEFT: Alignment = Alignment::new(-1.0, 1.0, 0.0);
    pub const BOTTOM_CENTER: Alignment = Alignment::new(0.0, 1.0, 0.0);
    pub const BOTTOM_RIGHT: Alignment = Alignment::new(1.0, 1.0, 0.0);
    pub const BACK: Alignment = Alignment::new(0.0, 0.0, -1.0);
    pub const FRONT: Alignment = Alignment::new(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Alignment {
        Alignment { x, y, z }
    }

    pub fn with_z(mut self, z: f32) -> Alignment {
        self.z = z;
        self
    }

    pub fn along(&self, axis: Axis) -> f32 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    /// The offset that aligns `child` inside `parent`.
    pub fn offset_of(&self, child: Size, parent: Size) -> Offset {
        Axis::ALL.iter().fold(Offset::ZERO, |offset, &axis| {
            let free = parent.along(axis) - child.along(axis);
            offset.with_axis(axis, free * (1.0 + self.along(axis)) / 2.0)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tight_and_loose() {
        let t = Constraints::tight(Size::new(10.0, 20.0, 3.0));
        assert!(t.is_tight());
        assert_eq!((t.min(Axis::Y), t.max(Axis::Z)), (20.0, 3.0));
        let l = Constraints::loose(Size::new(10.0, 20.0, 3.0));
        assert!(!l.is_tight() && l.min(Axis::X) == 0.0 && l.max(Axis::X) == 10.0);
        assert!(!Constraints::unbounded().has_bounded(Axis::X));
    }

    #[test]
    fn constrain_clamps_each_axis() {
        let c = Constraints::loose(Size::new(100.0, 50.0, 0.0));
        assert_eq!(
            c.constrain(Size::new(150.0, 20.0, 5.0)),
            Size::new(100.0, 20.0, 0.0)
        );
    }

    #[test]
    fn enforce_keeps_result_inside_other() {
        let r = Constraints::tight(Size::new(50.0, 50.0, 0.0))
            .enforce(Constraints::loose(Size::new(40.0, 100.0, 0.0)));
        assert_eq!(
            (r.min_width, r.max_width, r.min_height, r.max_height),
            (40.0, 40.0, 50.0, 50.0)
        );
        let c = Constraints::loose(Size::new(80.0, 60.0, 0.0));
        assert_eq!(
            Constraints::expand().enforce(c),
            Constraints::tight(c.biggest())
        );
    }

    #[test]
    fn deflate_subtracts_insets_without_going_negative() {
        let d = Constraints::tight(Size::new(100.0, 100.0, 10.0))
            .deflate(EdgeInsets::all(10.0).with_depth(2.0, 3.0));
        assert_eq!(d, Constraints::tight(Size::new(80.0, 80.0, 5.0)));
        let z = Constraints::loose(Size::new(5.0, 5.0, 0.0)).deflate(EdgeInsets::all(10.0));
        assert_eq!((z.max_width, z.min_width), (0.0, 0.0));
        assert_eq!(
            Constraints::unbounded()
                .deflate(EdgeInsets::all(10.0))
                .max_width,
            f32::INFINITY
        );
    }

    #[test]
    fn loosen_tighten_biggest_smallest() {
        let c = Constraints::tight(Size::new(10.0, 20.0, 0.0)).loosen();
        assert_eq!(c.smallest(), Size::ZERO);
        assert_eq!(c.biggest(), Size::new(10.0, 20.0, 0.0));
        assert_eq!(
            Constraints::loose(Size::new(100.0, 0.0, 0.0))
                .tighten(Axis::X, 150.0)
                .min_width,
            100.0
        );
        assert_eq!(
            Constraints::unbounded()
                .with_axis(Axis::Y, 5.0, 9.0)
                .max(Axis::Y),
            9.0
        );
    }

    #[test]
    fn size_and_offset_along_axes() {
        let s = Size::new(1.0, 2.0, 3.0);
        assert_eq!((s.along(Axis::X), s.along(Axis::Z)), (1.0, 3.0));
        assert_eq!(s.with_axis(Axis::Y, 9.0), Size::new(1.0, 9.0, 3.0));
        assert_eq!(Size::flat(4.0, 5.0), Size::new(4.0, 5.0, 0.0));
        assert_eq!(Offset::ZERO.with_axis(Axis::Z, 2.0).along(Axis::Z), 2.0);
    }

    #[test]
    fn edge_insets_all_is_planar() {
        let e = EdgeInsets::all(4.0);
        assert_eq!((e.horizontal(), e.vertical(), e.depth()), (8.0, 8.0, 0.0));
        assert_eq!(
            EdgeInsets::symmetric(1.0, 2.0).with_depth(3.0, 4.0).depth(),
            7.0
        );
        assert_eq!(EdgeInsets::ZERO.left(5.0).horizontal(), 5.0);
    }

    #[test]
    fn alignment_offsets_in_three_axes() {
        let (child, parent) = (Size::new(20.0, 10.0, 2.0), Size::new(100.0, 50.0, 10.0));
        assert_eq!(
            Alignment::CENTER.offset_of(child, parent),
            Offset::new(40.0, 20.0, 4.0)
        );
        assert_eq!(
            Alignment::BOTTOM_RIGHT.offset_of(child, parent),
            Offset::new(80.0, 40.0, 4.0)
        );
        assert_eq!(Alignment::FRONT.offset_of(child, parent).z, 8.0);
        assert_eq!(
            Alignment::TOP_LEFT.with_z(-1.0).offset_of(child, parent),
            Offset::ZERO
        );
    }

    #[test]
    fn invalid_constraints_never_panic_in_geometry() {
        let invalid = Constraints {
            min_width: 10.0,
            max_width: 5.0,
            ..Constraints::unbounded()
        };
        let _ = invalid.constrain(Size::flat(7.0, 0.0));
        let _ = Constraints::loose(Size::flat(100.0, 100.0)).enforce(invalid);
        let _ = invalid.tighten(Axis::X, 3.0);
        let nan = Constraints {
            max_height: f32::NAN,
            ..Constraints::unbounded()
        };
        let _ = nan.constrain(Size::flat(1.0, 1.0));
    }
}
