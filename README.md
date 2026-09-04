# wisp

A GPU user interface toolkit in Rust, built around the window every other
toolkit treats as an afterthought: one that is **transparent**, **always on
top**, lets clicks through every pixel its content is not drawn on, **moves
with that content**, and is paced by the display rather than by a timer.

The core underneath it is general purpose. The overlay case is what it is
for.

> **Status: early.** The renderer works and is tested. There is no text, no
> layout and no input yet — see [Where this is](#where-this-is) before
> depending on it.

```sh
cargo run --example cards -p wisp
```

## Why another one

Every Rust GUI toolkit assumes it owns a rectangle the operating system handed
it. That assumption is invisible until you want a character walking across
someone's desktop, a heads-up display over a game, a floating palette, or a
notification that is not a window-shaped box — and then it is in the way at
every level, from hit testing to the blend mode.

The awkward parts of that case are not obvious, and they are what this library
is made of. They were found the expensive way, in a desktop pet that shipped
each of these bugs first:

- **Positions are fractional, always.** A toolkit that floors an image's origin
  to a whole device pixel cannot move anything smoothly. It is not a rounding
  error you squint at; it is a visible stutter in anything that drifts slowly.
- **Points and device pixels are different types here.** Both are `f32` and only
  a name keeps them apart, so a doubled conversion does not crash — it draws at
  half size in the wrong quarter of the window. That shipped twice, in two
  files, written by someone who knew the difference.
- **Colours mix in Oklab.** Two colours averaged channel-wise in sRGB pass
  through a muddy band; blue to yellow goes grey.
- **The blend is premultiplied throughout.** Straight alpha darkens every
  antialiased edge against a transparent background, which is the only kind of
  background this library assumes.

## Where this is

| | |
|---|---|
| geometry, colour, the display list | **done** — `wisp-core`, 34 tests, no GPU or platform in it |
| the renderer: rounded rects, borders, gradients, shadows | **done** — one signed-distance field per quad, 9 tests that render and read the pixels back |
| a window, and a frame paced by the surface | **done** — `wisp::run` |
| text | not started. It will be `swash` + `cosmic-text`; the shaper is not going to be written here |
| layout | not started. `taffy` first, and something of its own only if taffy actually gets in the way |
| input, focus, IME | not started. Hangul composition is a requirement rather than a later chore |
| the overlay itself: click-through, hit masks, following its content | not started, and it is the whole point — everything above is the floor it needs |

The milestone that decides whether any of this worked is porting a real desktop
pet onto it. Until then the honest description is "a renderer with a window
around it".

## How it fits together

| crate | what it is | depends on |
|---|---|---|
| `wisp-core` | geometry, colour, the display list | nothing |
| `wisp-gpu` | the wgpu renderer | `wisp-core`, `wgpu` |
| `wisp` | the window, and the umbrella re-export | both |

`wisp-core` has no GPU and no platform in it, which is why nearly every test
can run anywhere and in milliseconds.

## Drawing

```rust
use wisp::{Background, Corners, Points, Quad, Rect, Rgba, WindowOptions, run};

run(WindowOptions::default(), |scene, frame| {
    let px = |v: f32| Points(v).to_device(frame.scale);
    scene.push(
        Quad::new(
            Rect::from_edges(px(40.0), px(40.0), px(280.0), px(190.0)),
            Background::Solid(Rgba::hex(0x1c1c24)),
        )
        .with_corners(Corners::all(px(12.0))),
    );
})
```

A rounded rectangle with a border and a shadow is one primitive, evaluated
once. Drawing a box and then its border as two quads is where the seam between
them comes from, and a shadow drawn as its own quad has a hard edge exactly
where it meets the thing casting it.

## Testing

```sh
cargo test --workspace
```

The renderer's tests draw to a texture and read the pixels back, because WGSL
is compiled by the driver at run time and a storage buffer's layout is agreed
by convention rather than by a compiler: a shader with a typo in it builds
perfectly and draws the wrong thing somewhere else in the frame. They skip
rather than fail where there is no adapter.

One of them is there because the first eight were not enough. They were written
with pure colours — `0x00` and `0xff` — which are the fixed points of the sRGB
transfer function, so they passed whether or not colours were linearised on the
way to the GPU. The window was rendering every mid-tone about twice as light as
it should and the suite stayed green. That was found by looking at it.

## License

MIT for the source. Not for artwork, fonts or audio distributed alongside it —
see [LICENSE](LICENSE).
