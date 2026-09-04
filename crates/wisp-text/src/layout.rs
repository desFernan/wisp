use std::collections::HashMap;

use cosmic_text::{Attrs, Buffer, FontSystem, Metrics, Shaping, SwashCache};
use wisp_core::geometry::{Point, Rect};
use wisp_core::scene::Masked;
use wisp_core::{DevicePixels, Rgba, Scene};

use crate::atlas::{Atlas, AtlasSlot};

/// Where a line sits in the space it was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    #[default]
    Start,
    Centre,
    End,
}

impl Align {
    fn to_cosmic(self) -> Option<cosmic_text::Align> {
        match self {
            // `None` rather than `Left`: left is not the start of every
            // script, and asking for it explicitly would put Arabic and Hebrew
            // on the wrong side of their own column.
            Self::Start => None,
            Self::Centre => Some(cosmic_text::Align::Center),
            Self::End => Some(cosmic_text::Align::End),
        }
    }
}

/// How heavy a face to ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    Bold,
}

impl Weight {
    fn to_cosmic(self) -> cosmic_text::Weight {
        match self {
            Self::Regular => cosmic_text::Weight::NORMAL,
            Self::Medium => cosmic_text::Weight::MEDIUM,
            Self::Bold => cosmic_text::Weight::BOLD,
        }
    }
}

/// What to set a run of text in.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// A family name, or `None` for whatever the system offers by default.
    pub family: Option<String>,
    pub weight: Weight,
    /// In device pixels, because that is what it is rasterised at. A caller
    /// working in points converts on the way in, which is the one place the
    /// scale factor belongs.
    pub size: DevicePixels,
    pub line_height: DevicePixels,
}

impl Font {
    pub fn new(size: DevicePixels) -> Self {
        Self {
            family: None,
            weight: Weight::Regular,
            size,
            // 1.4 is a reasonable default for reading and a bad one to leave
            // implicit, so it is written here rather than in the shaper.
            line_height: size * 1.4,
        }
    }

    pub fn weight(mut self, weight: Weight) -> Self {
        self.weight = weight;
        self
    }

    pub fn family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
}

/// One rasterised glyph, cached by everything that changes its shape.
///
/// The subpixel offset is part of the key. A glyph drawn at x = 10.0 and one
/// at x = 10.5 are different pictures, and caching only the first is how text
/// ends up snapping to whole pixels as it scrolls -- the exact stutter this
/// library exists to avoid, arrived at through the cache rather than through
/// the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GlyphKey {
    cache_key: cosmic_text::CacheKey,
}

/// Fonts, shaping and the glyph cache.
///
/// One of these is kept for the life of the application: it holds the system's
/// font list, which is expensive to build, and the atlas, which is the point.
pub struct TextSystem {
    fonts: FontSystem,
    swash: SwashCache,
    atlas: Atlas,
    cached: HashMap<GlyphKey, Option<(AtlasSlot, i32, i32)>>,
}

impl Default for TextSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl TextSystem {
    pub fn new() -> Self {
        Self {
            fonts: FontSystem::new(),
            swash: SwashCache::new(),
            // 1024 is four megabytes of coverage, which is a lot of text at
            // the sizes an interface uses.
            atlas: Atlas::new(1024),
            cached: HashMap::new(),
        }
    }

    pub fn atlas(&self) -> &Atlas {
        &self.atlas
    }

    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    /// How much room `text` would take, without drawing it.
    ///
    /// Shapes and throws the result away, which is what layout needs: a box
    /// has to be sized before there is anywhere to draw into. Glyphs are
    /// cached either way, so the shaping is not paid for twice.
    pub fn measure(
        &mut self,
        text: &str,
        font: &Font,
        wrap: Option<DevicePixels>,
    ) -> (DevicePixels, DevicePixels) {
        self.measure_aligned(text, font, wrap, Align::Start)
    }

    /// As [`Self::measure`], for text that is not set from the start.
    pub fn measure_aligned(
        &mut self,
        text: &str,
        font: &Font,
        wrap: Option<DevicePixels>,
        align: Align,
    ) -> (DevicePixels, DevicePixels) {
        let mut nowhere = Scene::new();
        self.draw_all(
            &mut nowhere,
            text,
            font,
            Point::new(DevicePixels::ZERO, DevicePixels::ZERO),
            wrap,
            Rgba::TRANSPARENT,
            None,
            align,
        )
    }

    /// Lays `text` out into `scene`, with `at` as the top left of the first
    /// line, and returns how much room it took.
    ///
    /// `wrap` is the width to break lines at, or `None` to let it run on.
    pub fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font: &Font,
        at: Point<DevicePixels>,
        wrap: Option<DevicePixels>,
        colour: Rgba,
    ) -> (DevicePixels, DevicePixels) {
        self.draw_all(scene, text, font, at, wrap, colour, None, Align::Start)
    }

    /// As [`Self::draw`], with nothing outside `clip` drawn.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_clipped(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font: &Font,
        at: Point<DevicePixels>,
        wrap: Option<DevicePixels>,
        colour: Rgba,
        clip: Option<wisp_core::geometry::Rect<DevicePixels>>,
    ) -> (DevicePixels, DevicePixels) {
        self.draw_all(scene, text, font, at, wrap, colour, clip, Align::Start)
    }

    /// The one that does the work.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_all(
        &mut self,
        scene: &mut Scene,
        text: &str,
        font: &Font,
        at: Point<DevicePixels>,
        wrap: Option<DevicePixels>,
        colour: Rgba,
        clip: Option<wisp_core::geometry::Rect<DevicePixels>>,
        align: Align,
    ) -> (DevicePixels, DevicePixels) {
        let metrics = Metrics::new(font.size.get(), font.line_height.get());
        let mut buffer = Buffer::new(&mut self.fonts, metrics);
        buffer.set_size(wrap.map(|w| w.get()), None);

        let mut attrs = Attrs::new().weight(font.weight.to_cosmic());
        if let Some(family) = font.family.as_deref() {
            attrs = attrs.family(cosmic_text::Family::Name(family));
        }
        // Advanced, not basic: basic shaping is a per-character lookup, which
        // is wrong for anything with ligatures or marks and silently wrong for
        // scripts that reorder.
        buffer.set_text(text, &attrs, Shaping::Advanced, align.to_cosmic());
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut used = (DevicePixels::ZERO, DevicePixels::ZERO);
        for run in buffer.layout_runs() {
            used.0 = used.0.max(DevicePixels(run.line_w));
            used.1 = DevicePixels(run.line_top + font.line_height.get());
            for glyph in run.glyphs {
                let physical = glyph.physical((at.x.get(), at.y.get() + run.line_y), 1.0);
                let Some((slot, left, top)) = self.glyph(physical.cache_key) else {
                    continue;
                };
                if slot.width == 0 {
                    continue;
                }
                let x = physical.x as f32 + left as f32;
                let y = physical.y as f32 - top as f32;
                scene.push_masked(Masked {
                    clip,
                    bounds: Rect::from_edges(
                        DevicePixels(x),
                        DevicePixels(y),
                        DevicePixels(x + slot.width as f32),
                        DevicePixels(y + slot.height as f32),
                    ),
                    uv: slot.uv(self.atlas.side()),
                    colour,
                });
            }
        }
        used
    }

    /// The atlas slot for one glyph, rasterising it the first time.
    ///
    /// `None` is remembered as well as `Some`: a glyph that could not be
    /// rasterised -- no outline, or an atlas with no room -- would otherwise be
    /// attempted again on every frame for as long as it is on screen.
    fn glyph(&mut self, key: cosmic_text::CacheKey) -> Option<(AtlasSlot, i32, i32)> {
        let entry = GlyphKey { cache_key: key };
        if let Some(cached) = self.cached.get(&entry) {
            return *cached;
        }
        let packed = self
            .swash
            .get_image(&mut self.fonts, key)
            .as_ref()
            .and_then(|image| {
                let (w, h) = (image.placement.width, image.placement.height);
                self.atlas
                    .add(w, h, &image.data)
                    .map(|slot| (slot, image.placement.left, image.placement.top))
            });
        self.cached.insert(entry, packed);
        packed
    }
}
