//! The design system, on one screen.
//!
//! Every text role at its own size and weight, every surface at its own
//! elevation, and the accent used once. If a step in either scale is too close
//! to its neighbour, this is where it shows.
//!
//! Run with `cargo run --example gallery`.

use wisp::{Edges, Element, Elevation, Role, Sizing, Theme, Ui, WindowOptions, column, row, text};

fn main() -> Result<(), winit::error::EventLoopError> {
    let mut dark = true;
    wisp::run(
        WindowOptions {
            title: "wisp — gallery".into(),
            size: (900.0, 620.0),
            clear: Theme::dark().base,
            transparent: false,
            ..Default::default()
        },
        move |ui: &mut Ui, _frame| {
            if ui.last().clicked("mode") {
                dark = !dark;
            }
            let theme = if dark { Theme::dark() } else { Theme::light() };

            column()
                .size(Sizing::Fill, Sizing::Fill)
                .background(theme.base)
                .padding(Edges::all(28.0))
                .gap(24.0)
                .child(
                    row()
                        .size(Sizing::Fill, Sizing::Hug)
                        .child(text("Design system", Role::Display, theme.ink))
                        .child(wisp::spacer())
                        .child(
                            row()
                                .padding(Edges::axes(7.0, 14.0))
                                .corners(8.0)
                                .background(if ui.last().hovered("mode") {
                                    theme.floating
                                } else {
                                    theme.raised
                                })
                                .border(1.0, theme.border)
                                .id("mode")
                                .child(text(
                                    if dark { "Dark" } else { "Light" },
                                    Role::Label,
                                    theme.ink,
                                )),
                        ),
                )
                .child(
                    row()
                        .size(Sizing::Fill, Sizing::Fill)
                        .gap(24.0)
                        .cross(wisp::Place::Stretch)
                        .child(type_scale(&theme))
                        .child(surfaces(&theme)),
                )
        },
    )
}

fn type_scale(theme: &Theme) -> Element {
    let mut list = column()
        .grow(1.0)
        .size(Sizing::Fill, Sizing::Fill)
        .padding(Edges::all(20.0))
        .gap(14.0)
        .corners(14.0)
        .border(1.0, theme.border)
        .background(theme.raised)
        .child(text("Type", Role::Title, theme.ink));

    for role in Role::EVERY {
        list = list.child(
            row()
                .size(Sizing::Fill, Sizing::Hug)
                .gap(14.0)
                .child(column().size(Sizing::Fixed(74.0), Sizing::Hug).child(text(
                    name(role),
                    Role::Caption,
                    theme.quiet,
                )))
                .child(text("The quick brown fox", role, theme.ink)),
        );
    }
    list
}

fn surfaces(theme: &Theme) -> Element {
    let mut list = column()
        .grow(1.0)
        .size(Sizing::Fill, Sizing::Fill)
        .padding(Edges::all(20.0))
        .gap(12.0)
        .corners(14.0)
        .border(1.0, theme.border)
        .background(theme.base)
        .child(text("Surfaces", Role::Title, theme.ink));

    for at in Elevation::EVERY {
        list = list.child(
            row()
                .size(Sizing::Fill, Sizing::Hug)
                .padding(Edges::all(14.0))
                .gap(10.0)
                .corners(10.0)
                .border(1.0, theme.border)
                .surface(theme, at)
                .child(text(elevation(at), Role::Label, theme.ink))
                .child(wisp::spacer())
                .child(text("Aa", Role::Body, theme.quiet)),
        );
    }

    list.child(wisp::spacer()).child(
        row()
            .size(Sizing::Fill, Sizing::Hug)
            .padding(Edges::all(14.0))
            .corners(10.0)
            .background(theme.accent)
            .child(text(
                "Accent — once per screen",
                Role::Label,
                theme.on_accent,
            )),
    )
}

fn name(role: Role) -> &'static str {
    match role {
        Role::Display => "Display",
        Role::Title => "Title",
        Role::Body => "Body",
        Role::Label => "Label",
        Role::Caption => "Caption",
    }
}

fn elevation(at: Elevation) -> &'static str {
    match at {
        Elevation::Sunk => "Sunk",
        Elevation::Base => "Base",
        Elevation::Raised => "Raised",
        Elevation::Floating => "Floating",
    }
}
