//! The interface wisp was built for, built with wisp.
//!
//! A chat window: a title bar, a list of conversations, a transcript, a
//! composer and a status line. Nothing here is a special case in the library --
//! it is rows, columns, text and the design system, and the whole frame is
//! rebuilt from this function every time the display asks for one.
//!
//! Run with `cargo run --example chat`.

use wisp::{
    Edges, Element, Elevation, Role, Sizing, Theme, Ui, WindowOptions, column, row, spacer, text,
};

const SIDEBAR: f32 = 224.0;
const TITLE_BAR: f32 = 40.0;
const STATUS_BAR: f32 = 28.0;
/// A line of prose stops being readable somewhere past ninety characters, so
/// the transcript is capped and centred rather than run across the window.
const READING: f32 = 720.0;

struct Message {
    from: &'static str,
    body: &'static str,
}

const TRANSCRIPT: &[Message] = &[
    Message {
        from: "you",
        body: "왜 라이트 테마에서 카드가 안 보였어?",
    },
    Message {
        from: "puck",
        body: "raised 와 floating 이 둘 다 흰색이었습니다. 밝은 테마는 위로 갈수록 흰색에 \
               가까워지니 아래 단계가 자리를 비켜줘야 합니다. 팔레트 테스트가 잡았습니다.",
    },
    Message {
        from: "you",
        body: "Show me the surface ramp.",
    },
];

fn main() -> Result<(), winit::error::EventLoopError> {
    let theme = Theme::dark();
    let mut dark = true;

    wisp::run(
        WindowOptions {
            title: "wisp — chat".into(),
            size: (1040.0, 680.0),
            clear: theme.base,
            transparent: false,
            ..Default::default()
        },
        move |ui: &mut Ui, _frame| {
            if ui.last().clicked("theme") {
                dark = !dark;
            }
            let theme = if dark { Theme::dark() } else { Theme::light() };

            column()
                .size(Sizing::Fill, Sizing::Fill)
                .background(theme.base)
                .child(title_bar(&theme))
                .child(
                    row()
                        .size(Sizing::Fill, Sizing::Hug)
                        .cross(wisp::Place::Stretch)
                        .grow(1.0)
                        .child(sidebar(&theme, ui))
                        .child(conversation(&theme, ui)),
                )
                .child(status_bar(&theme))
        },
    )
}

fn title_bar(theme: &Theme) -> Element {
    row()
        .size(Sizing::Fill, Sizing::Fixed(TITLE_BAR))
        .padding(Edges::axes(0.0, 16.0))
        .gap(10.0)
        .background(theme.sunk)
        .child(dot(theme.accent, 8.0))
        .child(text("Puck", Role::Title, theme.ink))
        .child(text("Untitled", Role::Caption, theme.quiet))
        .child(spacer())
        .child(pill("theme", "Theme", theme))
}

fn sidebar(theme: &Theme, ui: &Ui) -> Element {
    let mut list = column()
        .size(Sizing::Fixed(SIDEBAR), Sizing::Fill)
        .padding(Edges::all(12.0))
        .gap(4.0)
        .background(theme.sunk)
        .child(
            row()
                .size(Sizing::Fill, Sizing::Fixed(34.0))
                .centre()
                .corners(9.0)
                .border(1.0, theme.border)
                .background(if ui.last().hovered("new") {
                    theme.raised
                } else {
                    theme.base
                })
                .id("new")
                .child(text("New chat", Role::Label, theme.ink)),
        )
        .child(heading("Conversations", theme));

    for (index, title) in ["The surface ramp", "Sub-pixel text", "Overlay windows"]
        .into_iter()
        .enumerate()
    {
        let selected = index == 0;
        list = list.child(
            row()
                .size(Sizing::Fill, Sizing::Hug)
                .padding(Edges::axes(7.0, 10.0))
                .gap(8.0)
                .corners(8.0)
                .background(if selected {
                    theme.raised
                } else {
                    wisp::Rgba::TRANSPARENT
                })
                .child(dot(if selected { theme.accent } else { theme.border }, 6.0))
                .child(text(
                    title,
                    Role::Body,
                    if selected { theme.ink } else { theme.quiet },
                )),
        );
    }

    list.child(spacer()).child(
        column()
            .size(Sizing::Fill, Sizing::Hug)
            .padding(Edges::all(10.0))
            .gap(2.0)
            .corners(9.0)
            .background(theme.base)
            .child(text("Workspace", Role::Caption, theme.quiet))
            .child(text("puck-native", Role::Body, theme.ink)),
    )
}

fn conversation(theme: &Theme, ui: &Ui) -> Element {
    let mut thread = column()
        .size(Sizing::Fixed(READING), Sizing::Hug)
        .gap(18.0)
        .grow(1.0);

    for message in TRANSCRIPT {
        let mine = message.from == "you";
        thread = thread.child(
            column()
                .size(Sizing::Fill, Sizing::Hug)
                .gap(6.0)
                .child(text(
                    message.from,
                    Role::Label,
                    if mine { theme.accent } else { theme.quiet },
                ))
                .child(
                    column()
                        .size(Sizing::Fill, Sizing::Hug)
                        .padding(Edges::axes(11.0, 14.0))
                        .corners(12.0)
                        .background(if mine { theme.raised } else { theme.base })
                        .border(1.0, theme.border)
                        .child(text(message.body, Role::Body, theme.ink)),
                ),
        );
    }

    column()
        .grow(1.0)
        .size(Sizing::Fill, Sizing::Fill)
        .padding(Edges::axes(24.0, 24.0))
        .gap(20.0)
        .cross(wisp::Place::Centre)
        .child(thread)
        .child(composer(theme, ui))
}

fn composer(theme: &Theme, ui: &Ui) -> Element {
    column()
        .size(Sizing::Fixed(READING), Sizing::Hug)
        .padding(Edges::all(12.0))
        .gap(10.0)
        .corners(14.0)
        .border(1.0, theme.border)
        // The one lifted thing on the screen, and the one thing you are meant
        // to reach for. Two things claiming to be in front is neither of them
        // being in front.
        .surface(theme, Elevation::Floating)
        .child(text("Say something to Puck…", Role::Body, theme.quiet))
        .child(
            row()
                .size(Sizing::Fill, Sizing::Hug)
                .gap(8.0)
                .child(text("claude", Role::Caption, theme.quiet))
                .child(spacer())
                .child(
                    row()
                        .padding(Edges::axes(7.0, 14.0))
                        .corners(8.0)
                        .background(if ui.last().pressed("send") {
                            theme.accent.with_alpha(0.75)
                        } else {
                            theme.accent
                        })
                        .id("send")
                        .child(text("Send", Role::Label, theme.on_accent)),
                ),
        )
}

fn status_bar(theme: &Theme) -> Element {
    row()
        .size(Sizing::Fill, Sizing::Fixed(STATUS_BAR))
        .padding(Edges::axes(0.0, 16.0))
        .gap(8.0)
        .background(theme.sunk)
        .child(dot(theme.success, 6.0))
        .child(text("ready", Role::Caption, theme.quiet))
        .child(spacer())
        .child(text("3 messages", Role::Caption, theme.quiet))
}

fn heading(title: &'static str, theme: &Theme) -> Element {
    row()
        .padding(Edges {
            top: 14.0,
            bottom: 4.0,
            left: 10.0,
            right: 0.0,
        })
        .child(text(title, Role::Caption, theme.quiet))
}

fn dot(colour: wisp::Rgba, size: f32) -> Element {
    wisp::div()
        .size(Sizing::Fixed(size), Sizing::Fixed(size))
        .corners(size / 2.0)
        .background(colour)
}

fn pill(id: &'static str, label: &'static str, theme: &Theme) -> Element {
    row()
        .padding(Edges::axes(5.0, 12.0))
        .corners(8.0)
        .border(1.0, theme.border)
        .background(theme.base)
        .id(id)
        .child(text(label, Role::Caption, theme.quiet))
}
