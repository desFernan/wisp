//! Points, sizes and rectangles, each carrying the unit it is measured in.
//!
//! Generic over the unit so that a rectangle in points and a rectangle in
//! device pixels are different types. Mixing them is the mistake this crate is
//! shaped to prevent, and it is worth the type parameter to make it a compile
//! error rather than a thing you notice in a screenshot.

use crate::units::{DevicePixels, Points, Scale};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point<U> {
    pub x: U,
    pub y: U,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size<U> {
    pub width: U,
    pub height: U,
}

/// A rectangle, by its top-left corner and its size.
///
/// Top-left and downwards, which is what every drawing API in this library
/// uses. Converting from a platform that counts upwards from the bottom of a
/// display is the platform layer's job and happens once, at the edge.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect<U> {
    pub origin: Point<U>,
    pub size: Size<U>,
}

impl<U: Copy> Point<U> {
    pub fn new(x: U, y: U) -> Self {
        Self { x, y }
    }
}

impl<U: Copy> Size<U> {
    pub fn new(width: U, height: U) -> Self {
        Self { width, height }
    }
}

impl<U: Copy> Rect<U> {
    pub fn new(origin: Point<U>, size: Size<U>) -> Self {
        Self { origin, size }
    }
}

impl<U: Copy + std::ops::Sub<Output = U>> Rect<U> {
    /// Defined once for every unit rather than per unit, so that
    /// `Rect::from_edges(..)` is not ambiguous at the call site.
    pub fn from_edges(left: U, top: U, right: U, bottom: U) -> Self {
        Self {
            origin: Point::new(left, top),
            size: Size::new(right - left, bottom - top),
        }
    }
}

/// The four edges, for any unit -- including a plain `f32`, which is what
/// texture coordinates are measured in.
impl<U: Copy + std::ops::Add<Output = U>> Rect<U> {
    pub fn left(&self) -> U {
        self.origin.x
    }

    pub fn top(&self) -> U {
        self.origin.y
    }

    pub fn right(&self) -> U {
        self.origin.x + self.size.width
    }

    pub fn bottom(&self) -> U {
        self.origin.y + self.size.height
    }
}

macro_rules! rect_maths {
    ($unit:ty) => {
        impl Rect<$unit> {
            pub fn centre(&self) -> Point<$unit> {
                Point::new(
                    self.origin.x + self.size.width / 2.0,
                    self.origin.y + self.size.height / 2.0,
                )
            }

            /// Half-open on the right and bottom, so two rectangles laid edge
            /// to edge do not both claim the line between them.
            pub fn contains(&self, point: Point<$unit>) -> bool {
                point.x >= self.left()
                    && point.x < self.right()
                    && point.y >= self.top()
                    && point.y < self.bottom()
            }

            pub fn intersects(&self, other: &Self) -> bool {
                self.left() < other.right()
                    && other.left() < self.right()
                    && self.top() < other.bottom()
                    && other.top() < self.bottom()
            }

            /// The overlap, or `None` when there is not one.
            ///
            /// `None` rather than an empty rectangle: an empty rectangle
            /// invites being drawn, and a zero-sized draw call is a waste that
            /// looks like work.
            pub fn intersection(&self, other: &Self) -> Option<Self> {
                self.intersects(other).then(|| {
                    Self::from_edges(
                        self.left().max(other.left()),
                        self.top().max(other.top()),
                        self.right().min(other.right()),
                        self.bottom().min(other.bottom()),
                    )
                })
            }

            /// Grown on every side. A negative amount shrinks it, and it stops
            /// at empty rather than turning itself inside out.
            pub fn inset(&self, by: $unit) -> Self {
                let width = (self.size.width - by * 2.0).max(<$unit>::ZERO);
                let height = (self.size.height - by * 2.0).max(<$unit>::ZERO);
                Self {
                    origin: Point::new(self.origin.x + by, self.origin.y + by),
                    size: Size::new(width, height),
                }
            }

            pub fn is_empty(&self) -> bool {
                self.size.width <= <$unit>::ZERO || self.size.height <= <$unit>::ZERO
            }
        }
    };
}

rect_maths!(Points);
rect_maths!(DevicePixels);

impl Point<Points> {
    pub fn to_device(self, scale: Scale) -> Point<DevicePixels> {
        Point::new(self.x.to_device(scale), self.y.to_device(scale))
    }
}

impl Size<Points> {
    pub fn to_device(self, scale: Scale) -> Size<DevicePixels> {
        Size::new(self.width.to_device(scale), self.height.to_device(scale))
    }
}

impl Rect<Points> {
    pub fn to_device(self, scale: Scale) -> Rect<DevicePixels> {
        Rect::new(self.origin.to_device(scale), self.size.to_device(scale))
    }
}

impl Rect<DevicePixels> {
    pub fn to_points(self, scale: Scale) -> Rect<Points> {
        Rect::new(
            Point::new(
                self.origin.x.to_points(scale),
                self.origin.y.to_points(scale),
            ),
            Size::new(
                self.size.width.to_points(scale),
                self.size.height.to_points(scale),
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(l: f32, t: f32, r: f32, b: f32) -> Rect<Points> {
        Rect::from_edges(Points(l), Points(t), Points(r), Points(b))
    }

    #[test]
    fn a_rectangle_knows_its_edges() {
        let r = rect(10.0, 20.0, 40.0, 60.0);
        assert_eq!(r.left(), Points(10.0));
        assert_eq!(r.right(), Points(40.0));
        assert_eq!(r.size, Size::new(Points(30.0), Points(40.0)));
        assert_eq!(r.centre(), Point::new(Points(25.0), Points(40.0)));
    }

    #[test]
    fn the_shared_edge_belongs_to_exactly_one_of_two_neighbours() {
        // Otherwise a click on the boundary between two things hits both, and
        // which one wins depends on iteration order.
        let left = rect(0.0, 0.0, 10.0, 10.0);
        let right = rect(10.0, 0.0, 20.0, 10.0);
        let on_the_line = Point::new(Points(10.0), Points(5.0));
        assert!(!left.contains(on_the_line));
        assert!(right.contains(on_the_line));
    }

    #[test]
    fn rectangles_that_only_touch_do_not_intersect() {
        assert!(!rect(0.0, 0.0, 10.0, 10.0).intersects(&rect(10.0, 0.0, 20.0, 10.0)));
        assert!(rect(0.0, 0.0, 10.0, 10.0).intersects(&rect(9.0, 0.0, 20.0, 10.0)));
    }

    #[test]
    fn no_overlap_is_none_rather_than_an_empty_rectangle() {
        // An empty rectangle invites being drawn; None cannot be.
        assert_eq!(
            rect(0.0, 0.0, 5.0, 5.0).intersection(&rect(9.0, 9.0, 12.0, 12.0)),
            None
        );
        assert_eq!(
            rect(0.0, 0.0, 10.0, 10.0).intersection(&rect(5.0, 5.0, 20.0, 20.0)),
            Some(rect(5.0, 5.0, 10.0, 10.0))
        );
    }

    #[test]
    fn shrinking_past_nothing_stops_at_nothing() {
        // Rather than turning the rectangle inside out, which draws as a
        // rectangle somewhere else entirely.
        let squashed = rect(0.0, 0.0, 10.0, 10.0).inset(Points(20.0));
        assert!(squashed.is_empty());
        assert_eq!(squashed.size, Size::new(Points(0.0), Points(0.0)));
    }

    #[test]
    fn a_rectangle_converts_whole_and_keeps_its_fractions() {
        let scale = Scale::new(2.0).unwrap();
        let there = rect(1.5, 2.25, 11.5, 12.25).to_device(scale);
        assert_eq!(
            there.origin,
            Point::new(DevicePixels(3.0), DevicePixels(4.5))
        );
        assert_eq!(
            there.size,
            Size::new(DevicePixels(20.0), DevicePixels(20.0))
        );
        assert_eq!(there.to_points(scale), rect(1.5, 2.25, 11.5, 12.25));
    }
}
