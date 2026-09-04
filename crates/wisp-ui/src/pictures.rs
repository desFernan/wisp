//! Pictures: avatars, icons that are drawings, a character's sprite.
//!
//! Packed into one atlas and referred to by a handle, so that a window full of
//! icons is one texture and one draw call rather than a bind per picture.
//!
//! Deliberately not a path-keyed cache with reloading and eviction. A picture
//! is added once and kept, because everything a window draws repeatedly is
//! small and there are not many of them; a sprite sheet is added once at
//! startup and read every frame for the life of the process.

use std::collections::HashMap;

use etagere::{AtlasAllocator, size2};
use wisp_core::geometry::Rect;

/// A picture in the atlas.
///
/// Copied rather than borrowed, so that holding one does not borrow the atlas
/// for as long as the frame is being built.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    /// Where to read it, as 0..1 texture coordinates.
    pub uv: Rect<f32>,
    /// Its own size, in pixels, for anything that wants to draw it at its
    /// natural size or keep its proportions.
    pub size: (u32, u32),
}

/// Everything drawn from pixels rather than from a colour.
pub struct Pictures {
    allocator: AtlasAllocator,
    pixels: Vec<u8>,
    side: u32,
    dirty: Option<(u32, u32, u32, u32)>,
    by_name: HashMap<String, Picture>,
}

impl Default for Pictures {
    fn default() -> Self {
        // 2048 square of RGBA is sixteen megabytes, which is a lot of icons
        // and one generous sprite sheet.
        Self::new(2048)
    }
}

impl Pictures {
    pub fn new(side: u32) -> Self {
        Self {
            allocator: AtlasAllocator::new(size2(side as i32, side as i32)),
            pixels: vec![0; (side * side * 4) as usize],
            side,
            dirty: None,
            by_name: HashMap::new(),
        }
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The region written since this was last called.
    pub fn take_dirty(&mut self) -> Option<(u32, u32, u32, u32)> {
        self.dirty.take()
    }

    /// A picture that has already been added.
    pub fn get(&self, name: &str) -> Option<Picture> {
        self.by_name.get(name).copied()
    }

    /// Adds straight-alpha RGBA pixels under a name, or returns the one
    /// already there.
    ///
    /// `None` when the atlas is full. Full is a real state rather than an
    /// error: the caller draws without that picture for now and asks for a
    /// bigger atlas, instead of losing the frame.
    pub fn add(&mut self, name: &str, width: u32, height: u32, rgba: &[u8]) -> Option<Picture> {
        if let Some(already) = self.by_name.get(name) {
            return Some(*already);
        }
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return None;
        }
        // A pixel of padding, so that filtering at the edge of one picture
        // cannot reach into the next and leave a smear along its side.
        let slot = self
            .allocator
            .allocate(size2(width as i32 + 1, height as i32 + 1))?;
        let (x, y) = (slot.rectangle.min.x as u32, slot.rectangle.min.y as u32);

        for row in 0..height {
            let from = (row * width * 4) as usize;
            let to = (((y + row) * self.side + x) * 4) as usize;
            self.pixels[to..to + (width * 4) as usize]
                .copy_from_slice(&rgba[from..from + (width * 4) as usize]);
        }
        self.dirty = Some(match self.dirty {
            None => (x, y, x + width, y + height),
            Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x + width), b.max(y + height)),
        });

        let side = self.side as f32;
        let picture = Picture {
            uv: Rect::from_edges(
                x as f32 / side,
                y as f32 / side,
                (x + width) as f32 / side,
                (y + height) as f32 / side,
            ),
            size: (width, height),
        };
        self.by_name.insert(name.to_string(), picture);
        Some(picture)
    }

    /// Adds a PNG from disk.
    ///
    /// The name is the caller's, not the path: two paths can be the same
    /// picture and one path can be replaced underneath a running process.
    pub fn add_png(&mut self, name: &str, path: impl AsRef<std::path::Path>) -> Option<Picture> {
        if let Some(already) = self.by_name.get(name) {
            return Some(*already);
        }
        let file = std::fs::File::open(path).ok()?;
        let decoder = png::Decoder::new(std::io::BufReader::new(file));
        let mut reader = decoder.read_info().ok()?;
        let mut buffer = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buffer).ok()?;
        let rgba = match info.color_type {
            png::ColorType::Rgba => buffer[..info.buffer_size()].to_vec(),
            png::ColorType::Rgb => buffer[..info.buffer_size()]
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            // Anything else -- palettes, greyscale, sixteen bits -- would need
            // the expansion options set on the decoder. Refused rather than
            // guessed at, so a picture that does not appear says why.
            _ => return None,
        };
        self.add(name, info.width, info.height, &rgba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red(width: u32, height: u32) -> Vec<u8> {
        [255u8, 0, 0, 255].repeat((width * height) as usize)
    }

    #[test]
    fn a_picture_can_be_read_back_where_it_was_put() {
        let mut pictures = Pictures::new(64);
        let picture = pictures.add("one", 2, 2, &red(2, 2)).expect("room");
        assert_eq!(picture.size, (2, 2));
        let x = (picture.uv.left() * 64.0) as u32;
        let y = (picture.uv.top() * 64.0) as u32;
        let at = ((y * 64 + x) * 4) as usize;
        assert_eq!(&pictures.pixels()[at..at + 4], &[255, 0, 0, 255]);
    }

    #[test]
    fn the_same_name_twice_is_the_same_picture() {
        // Adding it again would pack a second copy and quietly halve the
        // atlas every time a window was rebuilt.
        let mut pictures = Pictures::new(64);
        let first = pictures.add("one", 2, 2, &red(2, 2)).unwrap();
        let second = pictures.add("one", 8, 8, &red(8, 8)).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn pictures_do_not_land_on_top_of_each_other() {
        let mut pictures = Pictures::new(64);
        let a = pictures.add("a", 8, 8, &red(8, 8)).unwrap();
        let b = pictures.add("b", 8, 8, &red(8, 8)).unwrap();
        assert_ne!(a.uv, b.uv);
    }

    #[test]
    fn a_full_atlas_says_so_rather_than_failing() {
        let mut pictures = Pictures::new(16);
        assert!(pictures.add("small", 8, 8, &red(8, 8)).is_some());
        assert!(pictures.add("huge", 64, 64, &red(64, 64)).is_none());
    }

    #[test]
    fn only_what_changed_is_reported_as_dirty() {
        let mut pictures = Pictures::new(64);
        pictures.add("one", 4, 4, &red(4, 4)).unwrap();
        let (l, t, r, b) = pictures.take_dirty().expect("something was written");
        assert!(r - l <= 5 && b - t <= 5);
        assert_eq!(pictures.take_dirty(), None);
    }

    #[test]
    fn pixels_that_are_not_there_are_refused() {
        // A caller that says 8x8 and hands over four pixels would otherwise
        // read past the end of its own buffer.
        let mut pictures = Pictures::new(64);
        assert!(pictures.add("short", 8, 8, &red(1, 1)).is_none());
        assert!(pictures.add("empty", 0, 0, &[]).is_none());
    }
}
