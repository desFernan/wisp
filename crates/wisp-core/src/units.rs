//! The two lengths a window deals in, kept apart by the type system.
//!
//! A window is laid out in **points** and drawn in **device pixels**, and on a
//! retina display one point is two of them. Both are `f32` at runtime, so
//! nothing but a name stops one being handed to something that wanted the
//! other -- and a wrong conversion does not crash. It draws, at half the size,
//! or in the wrong quarter of the window, which is a thing you find by looking
//! rather than by testing.
//!
//! This is not a hypothetical. Puck, the application wisp was extracted from,
//! shipped the same mistake twice in two files: a measurement that was already
//! in points, divided by the scale factor a second time. One folded a sidebar
//! away on every window anyone would open; the other told a character to climb
//! onto a ledge in the top-left quarter of a window, where there was nothing.
//! Both were written by someone who knew the difference and could not see it.
//!
//! So the conversions are the only way across, and they are named after what
//! they need: [`Points::to_device`] takes a [`Scale`], and there is no
//! `From<f32>` on either side of the fence.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// How many device pixels one point is worth on a particular display.
///
/// 1.0 on a display that is not scaled, 2.0 on a retina one. It cannot be
/// zero: a scale of zero collapses every length to nothing, and a window that
/// draws nothing at all is harder to diagnose than one that draws wrongly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale(f32);

impl Scale {
    /// `None` for a factor that is not a positive, finite number.
    pub fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(factor))
    }

    /// The scale of a display that is not scaled at all.
    pub const ONE: Self = Self(1.0);

    pub fn factor(self) -> f32 {
        self.0
    }
}

macro_rules! length {
    ($name:ident, $unit:literal) => {
        #[doc = concat!("A length in ", $unit, ".")]
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
        pub struct $name(pub f32);

        impl $name {
            pub const ZERO: Self = Self(0.0);

            pub fn get(self) -> f32 {
                self.0
            }

            pub fn min(self, other: Self) -> Self {
                Self(self.0.min(other.0))
            }

            pub fn max(self, other: Self) -> Self {
                Self(self.0.max(other.0))
            }

            pub fn abs(self) -> Self {
                Self(self.0.abs())
            }

            /// Clamped into `low..=high`. Panics if `low` is above `high`,
            /// which is a bug in the caller rather than a value to guess at.
            pub fn clamp(self, low: Self, high: Self) -> Self {
                assert!(low <= high, "clamp range is inverted: {low:?}..={high:?}");
                Self(self.0.clamp(low.0, high.0))
            }
        }

        impl Add for $name {
            type Output = Self;
            fn add(self, rhs: Self) -> Self {
                Self(self.0 + rhs.0)
            }
        }

        impl Sub for $name {
            type Output = Self;
            fn sub(self, rhs: Self) -> Self {
                Self(self.0 - rhs.0)
            }
        }

        impl Neg for $name {
            type Output = Self;
            fn neg(self) -> Self {
                Self(-self.0)
            }
        }

        /// Scaling by a bare number keeps the unit: half a length is still a
        /// length of the same kind.
        impl Mul<f32> for $name {
            type Output = Self;
            fn mul(self, rhs: f32) -> Self {
                Self(self.0 * rhs)
            }
        }

        impl Div<f32> for $name {
            type Output = Self;
            fn div(self, rhs: f32) -> Self {
                Self(self.0 / rhs)
            }
        }

        /// A ratio between two lengths of the same unit is a plain number.
        impl Div for $name {
            type Output = f32;
            fn div(self, rhs: Self) -> f32 {
                self.0 / rhs.0
            }
        }
    };
}

length!(Points, "points -- what layout is written in");
length!(DevicePixels, "device pixels -- what is actually drawn");

impl Points {
    /// The same length, measured in what will be drawn.
    pub fn to_device(self, scale: Scale) -> DevicePixels {
        DevicePixels(self.0 * scale.0)
    }
}

impl DevicePixels {
    /// The same length, measured in what layout is written in.
    pub fn to_points(self, scale: Scale) -> Points {
        Points(self.0 / scale.0)
    }

    /// Rounded to a whole pixel.
    ///
    /// Used for the edges of things that must not be blurry -- a one pixel
    /// border landing on a boundary is drawn as two grey ones. It is
    /// deliberately *not* applied to positions in general: a sprite that can
    /// only be placed on whole pixels cannot move smoothly, which is the
    /// stutter this library exists to avoid.
    pub fn round(self) -> Self {
        Self(self.0.round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(f: f32) -> Scale {
        Scale::new(f).expect("valid scale")
    }

    #[test]
    fn a_point_is_two_device_pixels_on_a_retina_display() {
        assert_eq!(Points(10.0).to_device(scale(2.0)), DevicePixels(20.0));
    }

    #[test]
    fn converting_there_and_back_is_the_length_you_started_with() {
        let there = Points(37.5).to_device(scale(2.0));
        assert_eq!(there.to_points(scale(2.0)), Points(37.5));
    }

    #[test]
    fn a_scale_of_zero_is_refused() {
        // Every length would collapse to nothing, and a window drawing nothing
        // at all is harder to work out than one drawing wrongly.
        assert!(Scale::new(0.0).is_none());
        assert!(Scale::new(-2.0).is_none());
        assert!(Scale::new(f32::NAN).is_none());
        assert!(Scale::new(f32::INFINITY).is_none());
    }

    #[test]
    fn arithmetic_stays_inside_one_unit() {
        assert_eq!(Points(3.0) + Points(4.0), Points(7.0));
        assert_eq!(Points(3.0) - Points(4.0), Points(-1.0));
        assert_eq!(Points(3.0) * 2.0, Points(6.0));
        assert_eq!(Points(3.0) / 2.0, Points(1.5));
    }

    #[test]
    fn dividing_two_lengths_gives_a_plain_ratio() {
        // Which is the one case where a unit should disappear: how many times
        // one length goes into another is a number, not a length.
        let ratio: f32 = Points(10.0) / Points(4.0);
        assert_eq!(ratio, 2.5);
    }

    #[test]
    fn rounding_is_available_but_not_automatic() {
        // The whole point of the library: a position may be fractional. Only
        // the caller decides when a value has to land on a pixel.
        assert_eq!(DevicePixels(10.4).round(), DevicePixels(10.0));
        assert_eq!(DevicePixels(10.6).round(), DevicePixels(11.0));
        let fractional = Points(10.3).to_device(scale(2.0));
        assert_eq!(
            fractional,
            DevicePixels(20.6),
            "not rounded on the way across"
        );
    }

    #[test]
    fn clamping_an_inverted_range_is_a_bug_not_a_guess() {
        let result = std::panic::catch_unwind(|| Points(1.0).clamp(Points(5.0), Points(2.0)));
        assert!(result.is_err());
    }
}
