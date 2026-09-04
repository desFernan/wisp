//! The display list: everything to be drawn this frame, in the order to draw
//! it, and nothing about how.
//!
//! A scene is built fresh each frame and handed to a renderer. It is plain
//! data with no GPU types in it, which is what lets the interesting parts --
//! what overlaps what, whether a shadow is inside its own bounds, whether a
//! radius larger than the box is handled -- be tested without a device.

use crate::colour::Rgba;
use crate::geometry::Rect;
use crate::units::DevicePixels;

/// A corner radius per corner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Corners {
    pub top_left: DevicePixels,
    pub top_right: DevicePixels,
    pub bottom_right: DevicePixels,
    pub bottom_left: DevicePixels,
}

impl Corners {
    pub const NONE: Self = Self {
        top_left: DevicePixels::ZERO,
        top_right: DevicePixels::ZERO,
        bottom_right: DevicePixels::ZERO,
        bottom_left: DevicePixels::ZERO,
    };

    pub fn all(radius: DevicePixels) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    /// Shrunk so that no pair of radii along an edge is wider than that edge.
    ///
    /// Two 40-pixel radii on a 50-pixel-wide box overlap, and a signed-distance
    /// field asked to draw that produces a shape with a pinched waist that
    /// nobody asked for. CSS solves it by scaling every radius by the same
    /// factor, which keeps the shape recognisable, and so does this.
    pub fn fitted_to(self, bounds: Rect<DevicePixels>) -> Self {
        let (w, h) = (bounds.size.width.get(), bounds.size.height.get());
        let ratio = [
            (self.top_left.get() + self.top_right.get(), w),
            (self.bottom_left.get() + self.bottom_right.get(), w),
            (self.top_left.get() + self.bottom_left.get(), h),
            (self.top_right.get() + self.bottom_right.get(), h),
        ]
        .into_iter()
        .filter(|(sum, _)| *sum > 0.0)
        .map(|(sum, edge)| (edge / sum).min(1.0))
        .fold(1.0f32, f32::min);

        Self {
            top_left: self.top_left * ratio,
            top_right: self.top_right * ratio,
            bottom_right: self.bottom_right * ratio,
            bottom_left: self.bottom_left * ratio,
        }
    }
}

/// What fills a quad.
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    Solid(Rgba),
    /// Along a line at `angle` radians, clockwise from pointing right.
    ///
    /// Stops carry their own position so that a gradient can be uneven; they
    /// are mixed in Oklab, which is [`Rgba::mix`]'s business rather than this
    /// type's.
    LinearGradient {
        angle: f32,
        stops: Vec<(f32, Rgba)>,
    },
}

impl Background {
    pub fn is_invisible(&self) -> bool {
        match self {
            Self::Solid(c) => c.is_transparent(),
            Self::LinearGradient { stops, .. } => {
                stops.is_empty() || stops.iter().all(|(_, c)| c.is_transparent())
            }
        }
    }

    /// The colour at `t` along the gradient, or the solid colour.
    ///
    /// Before the first stop and after the last it holds the end colour, which
    /// is what every other gradient in computing does and therefore what
    /// anybody writing one expects.
    pub fn sample(&self, t: f32) -> Rgba {
        match self {
            Self::Solid(c) => *c,
            Self::LinearGradient { stops, .. } => {
                let Some((first_at, first)) = stops.first().copied() else {
                    return Rgba::TRANSPARENT;
                };
                // Strictly before, so that a `t` sitting exactly on the first
                // stop falls through to the walk below. Two stops at the same
                // position are a hard edge, and at the edge itself the later
                // colour is the one that shows -- which is what CSS does, and
                // what anyone writing two stops at one position means.
                if t < first_at {
                    return first;
                }
                let (last_at, last) = stops.last().copied().expect("non-empty");
                if t >= last_at {
                    return last;
                }
                for pair in stops.windows(2) {
                    let ((a_at, a), (b_at, b)) = (pair[0], pair[1]);
                    if t >= a_at && t <= b_at {
                        let span = b_at - a_at;
                        // Two stops at the same position are a hard edge, and
                        // dividing by the gap between them is a division by
                        // zero. The later colour wins, as it does in CSS.
                        if span <= f32::EPSILON {
                            return b;
                        }
                        return a.mix(b, (t - a_at) / span);
                    }
                }
                last
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub width: DevicePixels,
    pub colour: Rgba,
}

/// A shadow cast by a quad, drawn behind it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub offset: (DevicePixels, DevicePixels),
    pub blur: DevicePixels,
    /// Grows or shrinks the shadow's shape before it is blurred.
    pub spread: DevicePixels,
    pub colour: Rgba,
}

impl Shadow {
    /// The area this shadow can reach, which is bigger than the quad casting
    /// it. A renderer that reserves only the quad's own bounds clips the
    /// shadow to a hard edge exactly where it should be softest.
    pub fn bounds_around(&self, quad: Rect<DevicePixels>) -> Rect<DevicePixels> {
        let reach = self.blur + self.spread;
        Rect::from_edges(
            quad.left() + self.offset.0 - reach,
            quad.top() + self.offset.1 - reach,
            quad.right() + self.offset.0 + reach,
            quad.bottom() + self.offset.1 + reach,
        )
    }
}

/// One rectangle: the only primitive this renderer has.
///
/// Rounded corners, a border and a shadow are all properties of it rather than
/// separate things to draw, because they are one signed-distance field
/// evaluated once. Two draw calls for a box and its border is where seams
/// between the two come from.
#[derive(Debug, Clone, PartialEq)]
pub struct Quad {
    /// Nothing outside this is drawn, shadow included.
    ///
    /// A rectangle rather than a stack of them: everything that scrolls in an
    /// interface is a rectangle, and a general clip path would be a second
    /// rasteriser to keep in step with the first.
    pub clip: Option<Rect<DevicePixels>>,
    pub bounds: Rect<DevicePixels>,
    pub background: Background,
    pub corners: Corners,
    pub border: Option<Border>,
    pub shadow: Option<Shadow>,
}

impl Quad {
    pub fn new(bounds: Rect<DevicePixels>, background: Background) -> Self {
        Self {
            clip: None,
            bounds,
            background,
            corners: Corners::NONE,
            border: None,
            shadow: None,
        }
    }

    pub fn with_corners(mut self, corners: Corners) -> Self {
        self.corners = corners;
        self
    }

    pub fn with_border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }

    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Nothing outside `clip` is drawn.
    pub fn clipped_to(mut self, clip: Option<Rect<DevicePixels>>) -> Self {
        self.clip = clip;
        self
    }

    /// Everything this quad can put pixels on, its shadow included.
    pub fn painted_bounds(&self) -> Rect<DevicePixels> {
        match self.shadow {
            Some(shadow) => {
                let s = shadow.bounds_around(self.bounds);
                Rect::from_edges(
                    self.bounds.left().min(s.left()),
                    self.bounds.top().min(s.top()),
                    self.bounds.right().max(s.right()),
                    self.bounds.bottom().max(s.bottom()),
                )
            }
            None => self.bounds,
        }
    }

    /// Whether drawing this would put any pixel anywhere.
    fn is_invisible(&self) -> bool {
        if !self.bounds.is_empty() {
            let border_shows = self
                .border
                .is_some_and(|b| b.width > DevicePixels::ZERO && !b.colour.is_transparent());
            let shadow_shows = self.shadow.is_some_and(|s| !s.colour.is_transparent());
            if !self.background.is_invisible() || border_shows || shadow_shows {
                return false;
            }
        }
        true
    }
}

/// A rectangle whose colour comes from a coverage mask rather than from a
/// fill: one glyph, one icon, anything drawn from an atlas.
///
/// The mask is a single channel -- how much of the pixel the shape covers --
/// and the colour is applied to it here. That is what lets one atlas serve
/// text in any colour without being redrawn per colour, which is most of why a
/// glyph cache is worth having.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Masked {
    /// Nothing outside this is drawn. `None` for no clipping.
    pub clip: Option<Rect<DevicePixels>>,
    /// Where it goes, in device pixels. Fractional, like everything else: a
    /// glyph snapped to a whole pixel is a line of text that jitters as it
    /// scrolls.
    pub bounds: Rect<DevicePixels>,
    /// Where to read it from in the atlas, as 0..1 texture coordinates.
    pub uv: Rect<f32>,
    pub colour: Rgba,
}

/// One frame's worth of drawing, back to front.
///
/// Two lists rather than one, because they are two draw calls: everything with
/// a fill, then everything read from the mask atlas. Text sits on top of the
/// boxes it is written in, which is the order anything with a background
/// wants, and it saves swapping pipelines per item.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    quads: Vec<Quad>,
    masked: Vec<Masked>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a quad, unless it could not put a pixel anywhere.
    ///
    /// Dropped here rather than in the renderer so that the saving is the same
    /// whichever renderer is used, and so that a scene can be asserted about in
    /// a test without a device.
    pub fn push(&mut self, quad: Quad) -> &mut Self {
        // A quad entirely outside its own clip cannot put a pixel anywhere, and
        // a scrolled list is mostly made of those.
        let clipped_away = quad
            .clip
            .is_some_and(|clip| !quad.painted_bounds().intersects(&clip));
        if !quad.is_invisible() && !clipped_away {
            self.quads.push(quad);
        }
        self
    }

    /// Adds something drawn from the mask atlas, unless it is invisible.
    pub fn push_masked(&mut self, masked: Masked) -> &mut Self {
        let clipped_away = masked
            .clip
            .is_some_and(|clip| !masked.bounds.intersects(&clip));
        if !masked.colour.is_transparent() && !masked.bounds.is_empty() && !clipped_away {
            self.masked.push(masked);
        }
        self
    }

    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    pub fn masked(&self) -> &[Masked] {
        &self.masked
    }

    pub fn is_empty(&self) -> bool {
        self.quads.is_empty() && self.masked.is_empty()
    }

    pub fn clear(&mut self) {
        self.quads.clear();
        self.masked.clear();
    }

    /// Everything this scene will put pixels on, or `None` for an empty one.
    pub fn painted_bounds(&self) -> Option<Rect<DevicePixels>> {
        let masked = self.masked.iter().map(|m| m.bounds);
        self.quads
            .iter()
            .map(Quad::painted_bounds)
            .chain(masked)
            .reduce(|a, b| {
                Rect::from_edges(
                    a.left().min(b.left()),
                    a.top().min(b.top()),
                    a.right().max(b.right()),
                    a.bottom().max(b.bottom()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};

    fn px(v: f32) -> DevicePixels {
        DevicePixels(v)
    }

    fn box_at(l: f32, t: f32, r: f32, b: f32) -> Rect<DevicePixels> {
        Rect::from_edges(px(l), px(t), px(r), px(b))
    }

    fn solid() -> Background {
        Background::Solid(Rgba::hex(0xed8c33))
    }

    #[test]
    fn an_invisible_quad_is_not_kept() {
        let mut scene = Scene::new();
        scene.push(Quad::new(
            box_at(0.0, 0.0, 10.0, 10.0),
            Background::Solid(Rgba::TRANSPARENT),
        ));
        scene.push(Quad::new(box_at(0.0, 0.0, 0.0, 10.0), solid()));
        assert!(scene.is_empty(), "{:?}", scene.quads());
    }

    #[test]
    fn a_transparent_fill_still_draws_when_it_has_a_border() {
        let mut scene = Scene::new();
        scene.push(
            Quad::new(
                box_at(0.0, 0.0, 10.0, 10.0),
                Background::Solid(Rgba::TRANSPARENT),
            )
            .with_border(Border {
                width: px(1.0),
                colour: Rgba::hex(0xffffff),
            }),
        );
        assert_eq!(scene.quads().len(), 1);
    }

    #[test]
    fn overlapping_radii_are_scaled_down_together() {
        // Not clamped one at a time: scaling them by one factor keeps the
        // shape, where clamping individually turns a pill into a lozenge.
        let bounds = box_at(0.0, 0.0, 50.0, 100.0);
        let fitted = Corners::all(px(40.0)).fitted_to(bounds);
        assert_eq!(fitted.top_left, px(25.0));
        assert_eq!(fitted.top_right, px(25.0));
        assert_eq!(fitted.bottom_left, px(25.0));
    }

    #[test]
    fn radii_that_already_fit_are_left_alone() {
        let bounds = box_at(0.0, 0.0, 100.0, 100.0);
        assert_eq!(
            Corners::all(px(8.0)).fitted_to(bounds),
            Corners::all(px(8.0))
        );
    }

    #[test]
    fn square_corners_survive_fitting() {
        // The `sum > 0` filter exists for this: every ratio would otherwise be
        // a division by zero.
        let bounds = box_at(0.0, 0.0, 10.0, 10.0);
        assert_eq!(Corners::NONE.fitted_to(bounds), Corners::NONE);
    }

    #[test]
    fn a_shadow_reaches_past_the_thing_casting_it() {
        // A renderer that reserves only the quad's bounds clips the shadow to
        // a hard edge exactly where it is supposed to be softest.
        let quad = Quad::new(box_at(10.0, 10.0, 20.0, 20.0), solid()).with_shadow(Shadow {
            offset: (px(0.0), px(4.0)),
            blur: px(8.0),
            spread: px(0.0),
            colour: Rgba::hex(0x000000).with_alpha(0.4),
        });
        let painted = quad.painted_bounds();
        assert_eq!(painted.left(), px(2.0));
        assert_eq!(painted.top(), px(6.0));
        assert_eq!(painted.bottom(), px(32.0));
    }

    #[test]
    fn a_gradient_holds_its_end_colours_outside_its_stops() {
        let start = Rgba::hex(0xed8c33);
        let end = Rgba::hex(0x3fb950);
        let g = Background::LinearGradient {
            angle: 0.0,
            stops: vec![(0.25, start), (0.75, end)],
        };
        assert_eq!(g.sample(0.0), start);
        assert_eq!(g.sample(-5.0), start);
        assert_eq!(g.sample(1.0), end);
        assert_eq!(g.sample(9.0), end);
    }

    #[test]
    fn two_stops_at_the_same_place_are_a_hard_edge_not_a_division_by_zero() {
        let a = Rgba::hex(0x000000);
        let b = Rgba::hex(0xffffff);
        let g = Background::LinearGradient {
            angle: 0.0,
            stops: vec![(0.5, a), (0.5, b)],
        };
        assert_eq!(g.sample(0.5), b);
        assert_eq!(g.sample(0.49), a);
    }

    #[test]
    fn a_gradient_with_no_stops_draws_nothing() {
        let g = Background::LinearGradient {
            angle: 0.0,
            stops: Vec::new(),
        };
        assert!(g.is_invisible());
        assert_eq!(g.sample(0.5), Rgba::TRANSPARENT);
    }

    #[test]
    fn a_scenes_bounds_cover_every_shadow_in_it() {
        let mut scene = Scene::new();
        scene.push(Quad::new(box_at(100.0, 100.0, 110.0, 110.0), solid()));
        scene.push(
            Quad::new(box_at(0.0, 0.0, 10.0, 10.0), solid()).with_shadow(Shadow {
                offset: (px(0.0), px(0.0)),
                blur: px(5.0),
                spread: px(0.0),
                colour: Rgba::hex(0x000000),
            }),
        );
        let bounds = scene.painted_bounds().expect("two quads");
        assert_eq!(bounds.left(), px(-5.0));
        assert_eq!(bounds.right(), px(110.0));
    }

    #[test]
    fn an_empty_scene_has_no_bounds_at_all() {
        assert_eq!(Scene::new().painted_bounds(), None);
    }

    #[test]
    fn a_quad_keeps_the_order_it_was_added_in() {
        // Back to front, and nothing sorts it: a renderer that reorders draws
        // is one that decides for itself what is on top.
        let mut scene = Scene::new();
        let first = Rect::new(Point::new(px(0.0), px(0.0)), Size::new(px(5.0), px(5.0)));
        let second = Rect::new(Point::new(px(9.0), px(9.0)), Size::new(px(5.0), px(5.0)));
        scene.push(Quad::new(first, solid()));
        scene.push(Quad::new(second, solid()));
        assert_eq!(scene.quads()[0].bounds, first);
        assert_eq!(scene.quads()[1].bounds, second);
    }
}
