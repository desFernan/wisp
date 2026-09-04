//! The interface wisp was built for, built with wisp.
//!
//! Run with `cargo run --example chat`.

use wisp::{Editor, OnEnter, Role, Theme, Ui, WindowOptions};

#[path = "common/mod.rs"]
mod common;

fn main() -> Result<(), winit::error::EventLoopError> {
    let theme = Theme::dark();
    let mut dark = true;
    let mut messages = common::opening();
    let mut composer = Editor::default();
    let mut opened = false;

    wisp::run(
        WindowOptions {
            title: "wisp \u{2014} chat".into(),
            size: (1040.0, 680.0),
            clear: theme.base,
            transparent: false,
            ..Default::default()
        },
        move |ui: &mut Ui, _frame| {
            // A chat window that opens with the keyboard somewhere else is a
            // chat window you have to click before you can use.
            if !opened {
                ui.focus("composer");
                opened = true;
            }
            if ui.last().clicked("theme") {
                dark = !dark;
            }
            let theme = if dark { Theme::dark() } else { Theme::light() };

            // The field is built and the keystrokes are applied in the same
            // call, so this has to happen before the tree that contains it.
            let (line, edited) = ui.field(
                "composer",
                &mut composer,
                &theme,
                Role::Body,
                "Say something to Puck\u{2026}",
                OnEnter::Submit,
            );
            let send = edited.submitted || ui.last().clicked("send");
            if send && !composer.text().trim().is_empty() {
                let said = composer.take();
                messages.push(common::Message {
                    from: "you".into(),
                    body: said.clone(),
                });
                messages.push(common::Message {
                    from: "puck".into(),
                    body: format!("You said {} characters. I am a mock.", said.chars().count()),
                });
            }

            common::window(&theme, ui, &messages, line)
        },
    )
}
