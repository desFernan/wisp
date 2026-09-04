//! A window that is not a rectangle somebody gave you.
//!
//! Transparent, over everything, and clicks pass straight through it wherever
//! nothing is drawn. The card catches them; the space around it does not.
//!
//!   cargo run --example overlay
//!
//! Move the pointer over the card and off it: the pill says which side of the
//! question the window is on, and the window is answering it sixty times a
//! second from what it just drew.

use wisp::{Edges, Elevation, Role, Sizing, Theme, Ui, WindowOptions, column, row, text};

fn main() -> Result<(), winit::error::EventLoopError> {
    let theme = Theme::dark();
    let mut clicks = 0u32;

    wisp::run(
        WindowOptions {
            title: "wisp — overlay".into(),
            size: (420.0, 260.0),
            // Nothing behind the card. Everything you can see through is the
            // desktop, and every pixel of it passes clicks along.
            clear: wisp::Rgba::TRANSPARENT,
            overlay: true,
            selftest: std::env::args().any(|a| a == "--selftest"),
            ..Default::default()
        },
        move |ui: &mut Ui, frame| {
            if ui.last().clicked("card") {
                clicks += 1;
            }
            let breathing = (frame.elapsed * 1.6).sin() * 0.5 + 0.5;

            column().size(Sizing::Fill, Sizing::Fill).centre().child(
                column()
                    .size(Sizing::Fixed(300.0), Sizing::Hug)
                    .padding(Edges::all(20.0))
                    .gap(10.0)
                    .corners(20.0)
                    .border(1.0, theme.border)
                    .surface(&theme, Elevation::Floating)
                    .id("card")
                    .child(text("wisp", Role::Display, theme.ink))
                    .child(text(
                        "Clicks land here. Anywhere outside this card they \
                             go to whatever is underneath.",
                        Role::Body,
                        theme.quiet,
                    ))
                    .child(
                        row()
                            .gap(8.0)
                            .child(
                                row()
                                    .padding(Edges::axes(5.0, 11.0))
                                    .corners(999.0)
                                    .background(if ui.last().hovered("card") {
                                        theme.accent
                                    } else {
                                        theme.raised
                                    })
                                    .child(text(
                                        if ui.last().hovered("card") {
                                            "catching"
                                        } else {
                                            "passing through"
                                        },
                                        Role::Caption,
                                        if ui.last().hovered("card") {
                                            theme.on_accent
                                        } else {
                                            theme.quiet
                                        },
                                    )),
                            )
                            .child(text(format!("{clicks} clicks"), Role::Caption, theme.quiet))
                            .child(wisp::spacer())
                            // Something moving, at fractional positions,
                            // so that the window is visibly alive rather
                            // than a still picture pinned over the desktop.
                            .child(
                                wisp::div()
                                    .size(
                                        Sizing::Fixed(8.0 + breathing * 6.0),
                                        Sizing::Fixed(8.0 + breathing * 6.0),
                                    )
                                    .corners(99.0)
                                    .background(theme.accent),
                            ),
                    ),
            )
        },
    )
}
