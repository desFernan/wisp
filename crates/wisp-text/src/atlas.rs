use etagere::{AtlasAllocator, size2};

/// Where one glyph sits in the atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasSlot {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AtlasSlot {
    /// The slot as texture coordinates, given the atlas it is in.
    pub fn uv(&self, atlas: u32) -> wisp_core::Rect<f32> {
        let side = atlas as f32;
        wisp_core::Rect::from_edges(
            self.x as f32 / side,
            self.y as f32 / side,
            (self.x + self.width) as f32 / side,
            (self.y + self.height) as f32 / side,
        )
    }
}

/// A single-channel atlas of coverage masks, packed as they are asked for.
///
/// One channel rather than four: a glyph is a shape, not a picture, and its
/// colour is applied when it is drawn. That is what lets the same cached glyph
/// serve text in every colour in the window.
pub struct Atlas {
    allocator: AtlasAllocator,
    pixels: Vec<u8>,
    side: u32,
    /// Which region has been written since the last time the caller asked, so
    /// that uploading does not mean sending the whole atlas every frame.
    dirty: Option<(u32, u32, u32, u32)>,
}

impl Atlas {
    /// A square atlas `side` pixels on a side.
    pub fn new(side: u32) -> Self {
        Self {
            allocator: AtlasAllocator::new(size2(side as i32, side as i32)),
            pixels: vec![0; (side * side) as usize],
            side,
            dirty: None,
        }
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Packs one mask, or `None` when the atlas is full.
    ///
    /// Full is a real state and not an error: the caller's answer is to drop
    /// the glyph for this frame rather than to fail the frame, and a bigger
    /// atlas next time.
    pub fn add(&mut self, width: u32, height: u32, mask: &[u8]) -> Option<AtlasSlot> {
        if width == 0 || height == 0 {
            // A space has no shape. It still has an advance, which is the
            // caller's business, but there is nothing to pack.
            return Some(AtlasSlot {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
        }
        // A pixel of padding, so that filtering at the edge of one glyph
        // cannot reach into the next one and leave a faint smear beside it.
        let slot = self
            .allocator
            .allocate(size2(width as i32 + 1, height as i32 + 1))?;
        let (x, y) = (slot.rectangle.min.x as u32, slot.rectangle.min.y as u32);

        for row in 0..height {
            let from = (row * width) as usize;
            let to = ((y + row) * self.side + x) as usize;
            self.pixels[to..to + width as usize]
                .copy_from_slice(&mask[from..from + width as usize]);
        }
        self.mark_dirty(x, y, width, height);
        Some(AtlasSlot {
            x,
            y,
            width,
            height,
        })
    }

    fn mark_dirty(&mut self, x: u32, y: u32, width: u32, height: u32) {
        self.dirty = Some(match self.dirty {
            None => (x, y, x + width, y + height),
            Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x + width), b.max(y + height)),
        });
    }

    /// The region written since this was last called, and clears it.
    pub fn take_dirty(&mut self) -> Option<(u32, u32, u32, u32)> {
        self.dirty.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mask_can_be_read_back_where_it_was_put() {
        let mut atlas = Atlas::new(64);
        let slot = atlas
            .add(2, 2, &[1, 2, 3, 4])
            .expect("room in an empty atlas");
        let at = |x: u32, y: u32| atlas.pixels()[((slot.y + y) * 64 + slot.x + x) as usize];
        assert_eq!([at(0, 0), at(1, 0), at(0, 1), at(1, 1)], [1, 2, 3, 4]);
    }

    #[test]
    fn glyphs_do_not_land_on_top_of_each_other() {
        let mut atlas = Atlas::new(64);
        let a = atlas.add(8, 8, &[255; 64]).unwrap();
        let b = atlas.add(8, 8, &[255; 64]).unwrap();
        let overlaps = a.x < b.x + b.width
            && b.x < a.x + a.width
            && a.y < b.y + b.height
            && b.y < a.y + a.height;
        assert!(!overlaps, "{a:?} and {b:?} overlap");
    }

    #[test]
    fn there_is_a_gap_between_neighbours() {
        // Without it, filtering at the edge of one glyph samples the next and
        // leaves a faint smear beside every letter.
        let mut atlas = Atlas::new(64);
        let a = atlas.add(8, 8, &[255; 64]).unwrap();
        let b = atlas.add(8, 8, &[255; 64]).unwrap();
        let apart = (a.x as i64 - b.x as i64).abs() > a.width as i64
            || (a.y as i64 - b.y as i64).abs() > a.height as i64;
        assert!(apart, "{a:?} and {b:?} are touching");
    }

    #[test]
    fn a_full_atlas_says_so_rather_than_failing() {
        // The caller drops a glyph for one frame and asks for a bigger atlas;
        // it does not lose the frame.
        let mut atlas = Atlas::new(16);
        assert!(atlas.add(8, 8, &[0; 64]).is_some());
        assert!(atlas.add(64, 64, &[0; 4096]).is_none());
    }

    #[test]
    fn a_space_has_no_shape_to_pack() {
        let mut atlas = Atlas::new(64);
        let slot = atlas
            .add(0, 0, &[])
            .expect("an empty glyph is not a failure");
        assert_eq!((slot.width, slot.height), (0, 0));
    }

    #[test]
    fn only_what_changed_is_reported_as_dirty() {
        // Uploading the whole atlas every frame is most of the cost of a glyph
        // cache and all of the reason not to have one.
        let mut atlas = Atlas::new(64);
        atlas.add(4, 4, &[255; 16]).unwrap();
        let (l, t, r, b) = atlas.take_dirty().expect("something was written");
        assert!(r - l <= 5 && b - t <= 5, "dirty region is the whole atlas");
        assert_eq!(atlas.take_dirty(), None, "taking it twice reports it twice");
    }

    #[test]
    fn texture_coordinates_are_the_slot_over_the_side() {
        let slot = AtlasSlot {
            x: 16,
            y: 32,
            width: 8,
            height: 8,
        };
        let uv = slot.uv(64);
        assert_eq!(uv.left(), 0.25);
        assert_eq!(uv.top(), 0.5);
        assert_eq!(uv.right(), 0.375);
    }
}
