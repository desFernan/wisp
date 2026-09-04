//! Shaping real text with the real fonts on this machine.
//!
//! Everything here goes through the system's font list, so these are about the
//! wiring rather than about exact glyph shapes: that a string becomes glyphs,
//! that the glyphs are in the right order and place, that the cache does not
//! change the answer, and that a script the default font has never heard of
//! still comes out as something.

use wisp_core::geometry::Point;
use wisp_core::{DevicePixels, Rgba, Scene};
use wisp_text::{Font, TextSystem, Weight};

fn draw(system: &mut TextSystem, text: &str, font: &Font) -> Scene {
    let mut scene = Scene::new();
    system.draw(
        &mut scene,
        text,
        font,
        Point::new(DevicePixels(10.0), DevicePixels(10.0)),
        None,
        Rgba::hex(0xffffff),
    );
    scene
}

#[test]
fn a_word_becomes_one_glyph_per_letter() {
    let mut system = TextSystem::new();
    let scene = draw(&mut system, "wisp", &Font::new(DevicePixels(32.0)));
    assert_eq!(scene.masked().len(), 4, "expected one glyph per letter");
}

#[test]
fn glyphs_march_left_to_right() {
    let mut system = TextSystem::new();
    let scene = draw(&mut system, "abc", &Font::new(DevicePixels(32.0)));
    let xs: Vec<f32> = scene
        .masked()
        .iter()
        .map(|g| g.bounds.left().get())
        .collect();
    assert!(
        xs.windows(2).all(|w| w[0] < w[1]),
        "glyphs are out of order: {xs:?}"
    );
}

#[test]
fn a_space_takes_room_without_drawing_anything() {
    // It has an advance and no shape. A renderer handed a zero-sized quad for
    // it would be drawing nothing, slowly.
    let mut system = TextSystem::new();
    let with = draw(&mut system, "a b", &Font::new(DevicePixels(32.0)));
    assert_eq!(with.masked().len(), 2, "the space should not be a glyph");

    let b_with_space = with.masked()[1].bounds.left();
    let without = draw(&mut system, "ab", &Font::new(DevicePixels(32.0)));
    assert!(
        b_with_space > without.masked()[1].bounds.left(),
        "the space took no room"
    );
}

#[test]
fn the_same_text_twice_lays_out_the_same_way() {
    // The second pass reads every glyph from the cache instead of rasterising
    // it. If the cache key were missing anything -- the size, the weight, the
    // subpixel offset -- this is where it would show.
    let mut system = TextSystem::new();
    let font = Font::new(DevicePixels(24.0));
    let first = draw(&mut system, "cached", &font);
    let second = draw(&mut system, "cached", &font);
    assert_eq!(first.masked(), second.masked());
}

#[test]
fn a_bigger_size_takes_more_room() {
    let mut system = TextSystem::new();
    let small = draw(&mut system, "size", &Font::new(DevicePixels(12.0)));
    let large = draw(&mut system, "size", &Font::new(DevicePixels(48.0)));
    let width = |s: &Scene| {
        let l = s.masked().first().unwrap().bounds.left().get();
        let r = s.masked().last().unwrap().bounds.right().get();
        r - l
    };
    assert!(
        width(&large) > width(&small) * 2.0,
        "48px is not much wider than 12px"
    );
}

#[test]
fn bold_is_not_the_same_as_regular() {
    // A weight that is asked for and silently ignored is the sort of thing
    // that looks fine until someone compares two labels.
    let mut system = TextSystem::new();
    let regular = draw(&mut system, "weight", &Font::new(DevicePixels(32.0)));
    let bold = draw(
        &mut system,
        "weight",
        &Font::new(DevicePixels(32.0)).weight(Weight::Bold),
    );
    assert_ne!(regular.masked(), bold.masked());
}

#[test]
fn a_script_the_default_font_does_not_have_still_comes_out() {
    // Fallback is the part of text that is invisible when it works and is a
    // row of empty boxes when it does not.
    let mut system = TextSystem::new();
    let scene = draw(&mut system, "한글", &Font::new(DevicePixels(32.0)));
    assert!(
        !scene.masked().is_empty(),
        "Hangul produced no glyphs at all"
    );
}

#[test]
fn wrapping_puts_the_rest_on_another_line() {
    let mut system = TextSystem::new();
    let font = Font::new(DevicePixels(16.0));
    let mut scene = Scene::new();
    system.draw(
        &mut scene,
        "a sentence long enough that it cannot fit on one short line",
        &font,
        Point::new(DevicePixels(0.0), DevicePixels(0.0)),
        Some(DevicePixels(120.0)),
        Rgba::hex(0xffffff),
    );
    let tops: Vec<f32> = scene
        .masked()
        .iter()
        .map(|g| g.bounds.top().get())
        .collect();
    let lowest = tops.iter().cloned().fold(f32::MIN, f32::max);
    let highest = tops.iter().cloned().fold(f32::MAX, f32::min);
    assert!(lowest - highest > 10.0, "everything landed on one line");
}

#[test]
fn a_fraction_of_a_pixel_changes_what_is_drawn() {
    // The library's whole argument, arrived at the way text has to arrive at
    // it. A glyph is not placed at a fractional position -- it is rasterised
    // *with* the fraction baked into its coverage, and placed on a whole
    // pixel. Same idea as the sprite compositor this library came from: the
    // fraction has to live somewhere that will not be rounded, and the only
    // such place is the pixels.
    //
    // So the check is not that the position has a fraction in it. It is that
    // the fraction reaches the rasteriser at all: a cache key missing its
    // subpixel bin would hand back the same picture for both of these, and
    // text would snap to whole pixels as it scrolled.
    let mut system = TextSystem::new();
    let font = Font::new(DevicePixels(17.0));
    let at = |system: &mut TextSystem, x: f32| {
        let mut scene = Scene::new();
        system.draw(
            &mut scene,
            "o",
            &font,
            Point::new(DevicePixels(x), DevicePixels(10.0)),
            None,
            Rgba::hex(0xffffff),
        );
        scene.masked()[0].uv
    };
    let whole = at(&mut system, 10.0);
    let fractional = at(&mut system, 10.5);
    assert_ne!(
        whole, fractional,
        "the same glyph came back for two different subpixel positions"
    );
}
