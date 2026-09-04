//! Colour, and the one thing about it that is easy to get wrong: blending.
//!
//! Two colours mixed channel by channel in sRGB pass through a muddy band on
//! the way. Blue to yellow goes grey in the middle; orange to blue goes brown.
//! It is not subtle and it is what makes a hand-written gradient look cheap.
//!
//! So mixing happens in Oklab, which was designed for exactly this: a
//! perceptual space where the straight line between two colours is the one the
//! eye expects. Storage stays sRGB, because that is what everything else --
//! themes, CSS, designers, the GPU's framebuffer -- speaks.

/// A colour in sRGB, with straight (not premultiplied) alpha, 0..=1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// `0xRRGGBB`, opaque. The form a theme is usually written in.
    pub fn hex(rgb: u32) -> Self {
        Self {
            r: ((rgb >> 16) & 0xff) as f32 / 255.0,
            g: ((rgb >> 8) & 0xff) as f32 / 255.0,
            b: (rgb & 0xff) as f32 / 255.0,
            a: 1.0,
        }
    }

    pub fn with_alpha(self, a: f32) -> Self {
        Self {
            a: a.clamp(0.0, 1.0),
            ..self
        }
    }

    pub fn is_transparent(self) -> bool {
        self.a <= 0.0
    }

    /// The same colour with the sRGB transfer function undone.
    ///
    /// The channels of an `Rgba` are gamma-encoded, which is what a theme, a
    /// designer and CSS all mean by a colour. A GPU writing into an sRGB
    /// framebuffer does the encoding itself, so what a shader hands it has to
    /// be linear -- passing the encoded values straight through brightens
    /// everything, and does it invisibly for pure colours because 0 and 1 are
    /// fixed points of the curve.
    pub fn to_linear(self) -> Self {
        Self {
            r: to_linear(self.r),
            g: to_linear(self.g),
            b: to_linear(self.b),
            a: self.a,
        }
    }

    /// `t` of the way from this colour to `other`, mixed in Oklab.
    ///
    /// Alpha is mixed straight: it is a coverage, not a colour, and it has no
    /// perceptual space to be wrong in.
    pub fn mix(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let (a, b) = (Oklab::from(self), Oklab::from(other));
        let mixed = Oklab {
            l: a.l + (b.l - a.l) * t,
            a: a.a + (b.a - a.a) * t,
            b: a.b + (b.b - a.b) * t,
        };
        let mut out = Rgba::from(mixed);
        out.a = self.a + (other.a - self.a) * t;
        out
    }
}

/// Oklab: a perceptual space, used here only as somewhere to blend.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Oklab {
    l: f32,
    a: f32,
    b: f32,
}

/// sRGB's transfer function, and its inverse.
///
/// The channels stored in a colour are gamma-encoded. Averaging two encoded
/// values is not the average of the light they stand for, which is the whole
/// reason a naive blend darkens.
fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

impl From<Rgba> for Oklab {
    fn from(c: Rgba) -> Self {
        let (r, g, b) = (to_linear(c.r), to_linear(c.g), to_linear(c.b));
        let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
        let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
        let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_5 * b).cbrt();
        Self {
            l: 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
            a: 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
            b: 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
        }
    }
}

impl From<Oklab> for Rgba {
    fn from(c: Oklab) -> Self {
        let l = (c.l + 0.396_337_78 * c.a + 0.215_803_76 * c.b).powi(3);
        let m = (c.l - 0.105_561_346 * c.a - 0.063_854_17 * c.b).powi(3);
        let s = (c.l - 0.089_484_18 * c.a - 1.291_485_5 * c.b).powi(3);
        Self {
            r: from_linear(4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s).clamp(0.0, 1.0),
            g: from_linear(-1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s).clamp(0.0, 1.0),
            b: from_linear(-0.004_196_086 * l - 0.703_418_6 * m + 1.707_614_7 * s).clamp(0.0, 1.0),
            a: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn hex_reads_the_channels_in_the_order_people_write_them() {
        let orange = Rgba::hex(0xed8c33);
        assert!(close(orange.r, 0.929) && close(orange.g, 0.549) && close(orange.b, 0.200));
        assert_eq!(orange.a, 1.0);
    }

    #[test]
    fn a_colour_survives_the_trip_through_oklab() {
        for hex in [0x000000, 0xffffff, 0xed8c33, 0x3fb950, 0x101014] {
            let there = Rgba::hex(hex);
            let back = Rgba::from(Oklab::from(there));
            assert!(
                close(there.r, back.r) && close(there.g, back.g) && close(there.b, back.b),
                "{hex:06x} came back as {back:?}"
            );
        }
    }

    #[test]
    fn the_ends_of_a_mix_are_the_colours_you_asked_for() {
        let a = Rgba::hex(0xed8c33);
        let b = Rgba::hex(0x3fb950);
        for (t, want) in [(0.0, a), (1.0, b)] {
            let got = a.mix(b, t);
            assert!(close(got.r, want.r) && close(got.g, want.g) && close(got.b, want.b));
        }
    }

    #[test]
    fn blue_to_yellow_does_not_go_grey_in_the_middle() {
        // The whole reason for mixing in Oklab. Averaged in sRGB the midpoint
        // of these two is a desaturated sludge; perceptually it should stay a
        // colour.
        let middle = Rgba::hex(0x0000ff).mix(Rgba::hex(0xffff00), 0.5);
        let greyness = {
            let max = middle.r.max(middle.g).max(middle.b);
            let min = middle.r.min(middle.g).min(middle.b);
            1.0 - (max - min)
        };
        assert!(greyness < 0.75, "midpoint is washed out: {middle:?}");
    }

    #[test]
    fn alpha_is_mixed_straight() {
        // It is coverage rather than colour, so it has no perceptual space to
        // be wrong in -- and running it through one would make a fade
        // non-linear for no reason.
        let clear = Rgba::hex(0xffffff).with_alpha(0.0);
        let solid = Rgba::hex(0xffffff).with_alpha(1.0);
        assert!(close(clear.mix(solid, 0.25).a, 0.25));
    }

    #[test]
    fn pure_black_and_white_survive_linearising_unchanged() {
        // Which is exactly why a missing conversion is easy to miss: they are
        // the fixed points of the curve, so any test written with them passes
        // either way.
        assert_eq!(Rgba::hex(0x000000).to_linear().r, 0.0);
        assert_eq!(Rgba::hex(0xffffff).to_linear().r, 1.0);
    }

    #[test]
    fn a_mid_tone_gets_much_darker_when_it_is_linearised() {
        // Mid grey is the case that shows it: 0x80 is a little over half in
        // sRGB and a little under a quarter of the actual light.
        let mid = Rgba::hex(0x808080);
        assert!(close(mid.r, 0.502));
        assert!(close(mid.to_linear().r, 0.216), "{}", mid.to_linear().r);
    }

    #[test]
    fn linearising_leaves_alpha_alone() {
        // It is coverage. There is no transfer function on how much of a thing
        // there is.
        assert_eq!(Rgba::hex(0x808080).with_alpha(0.4).to_linear().a, 0.4);
    }

    #[test]
    fn a_mix_outside_the_ends_is_clamped_to_them() {
        let a = Rgba::hex(0x000000);
        let b = Rgba::hex(0xffffff);
        assert_eq!(a.mix(b, -1.0), a.mix(b, 0.0));
        assert_eq!(a.mix(b, 2.0), a.mix(b, 1.0));
    }
}
