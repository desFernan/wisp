# wisp

A GPU user interface toolkit in Rust, built around the window every other
toolkit treats as an afterthought: one that is **transparent**, **always on
top**, lets clicks through every pixel its content is not drawn on, **moves
with that content**, and is paced by the display rather than by a timer.

The core underneath it is general purpose. The overlay case is what it is
for.

> **Status: early.** You can build a window with it: layout, text, pointer
> input and a design system all work and are tested. There is no text *entry*
> yet, no scrolling, and — the thing it is named for — no overlay. See
> [Where this is](#where-this-is) before depending on it.

```sh
cargo run --example chat -p wisp      # a chat window, built out of the toolkit
cargo run --example gallery -p wisp   # the type scale and the surface ramp
```

62 tests.

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
| geometry, colour, the display list | **done** — `wisp-core`, no GPU or platform in it |
| the renderer: rounded rects, borders, gradients, shadows, glyphs | **done** — one signed-distance field per quad, and a coverage atlas for everything cached. 12 tests render and read the pixels back |
| a window, and a frame paced by the surface | **done** — `wisp::run` |
| text | **shaping, a glyph cache and an atlas** — `wisp-text`, on `cosmic-text` and `swash`. No shaper of its own and there will not be one |
| layout | **done** — flexbox on `taffy`, with `Fill` meaning *the space that is left* along an axis and *all of it* across one, which is the distinction a sidebar beside a pane lives or dies on |
| a design system | **done** — a named type scale and surface ramp, with tests that neighbouring steps stay far enough apart to be seen. Most toolkits leave this to the application and most applications never get to it |
| pointer input | **done** — hover, press and click, hit-tested against the frame that was drawn. A click is a press and a release on the same box, so sliding off one and letting go cancels |
| text entry, focus, IME | not started, and the hardest thing left. Hangul composition is a requirement rather than a later chore |
| scrolling | not started |
| the overlay itself: click-through, hit masks, following its content | not started, and it is the whole point — everything above is the floor it needs |

The milestone that decides whether any of this worked is porting a real desktop
pet onto it. Until then the honest description is "a renderer with a window
around it".

## How it fits together

| crate | what it is | depends on |
|---|---|---|
| `wisp-core` | geometry, colour, the display list | nothing |
| `wisp-gpu` | the wgpu renderer | `wisp-core`, `wgpu` |
| `wisp-text` | shaping, rasterising, the glyph atlas | `wisp-core`, `cosmic-text` |
| `wisp-ui` | layout, pointer input, the design system | `wisp-core`, `wisp-text`, `taffy` |
| `wisp` | the window, and the umbrella re-export | all of them |

`wisp-core` has no GPU and no platform in it, which is why nearly every test
can run anywhere and in milliseconds.

## Building a window

```rust
use wisp::{Edges, Elevation, Role, Sizing, Theme, Ui, WindowOptions, column, row, text};

let theme = Theme::dark();
wisp::run(WindowOptions::default(), move |ui: &mut Ui, _frame| {
    if ui.last().clicked("send") {
        // ...
    }
    column()
        .size(Sizing::Fill, Sizing::Fill)
        .padding(Edges::all(24.0))
        .gap(12.0)
        .background(theme.base)
        .child(text("Say something", Role::Title, theme.ink))
        .child(
            row()
                .padding(Edges::axes(8.0, 16.0))
                .corners(8.0)
                .background(theme.accent)
                .id("send")
                .child(text("Send", Role::Label, theme.on_accent)),
        )
})
```

The tree is rebuilt every frame and nothing in it remembers anything. A button
is a box with a name; whether it was clicked is a question about the frame that
has already been drawn. There is no widget state to keep in step with the
application's own, which is the bug retained toolkits spend their lives on.

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
see [LICENSE-ASSETS.md](LICENSE-ASSETS.md).
