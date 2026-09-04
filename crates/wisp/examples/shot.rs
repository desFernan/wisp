//! Renders the chat window to a PNG with no window at all.
//!
//!   cargo run --example shot -- out.png
//!
//! The same layout, renderer and fonts a window would use. It exists because
//! looking at a window needs a machine with a screen and a window nothing is
//! covering, and neither is true often enough to rely on.

use wisp::{Editor, OnEnter, Role, Theme, Ui};

#[path = "common/mod.rs"]
mod common;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "wisp.png".into());
    let theme = Theme::dark();
    let mut editor = Editor::new(
        "\u{d55c}\u{ae00}\u{b3c4} \u{c870}\u{d569}\u{d574}\u{c11c} \u{b4e4}\u{c5b4}\u{ac11}\u{b2c8}\u{b2e4}",
    );
    let messages = common::opening();
    wisp::snapshot::write(&path, (1040.0, 680.0), 2.0, theme.base, |ui: &mut Ui| {
        ui.focus("composer");
        let (line, _) = ui.field(
            "composer",
            &mut editor,
            &theme,
            Role::Body,
            "Say something to Puck\u{2026}",
            OnEnter::Submit,
        );
        common::window(&theme, ui, &messages, line)
    })?;
    println!("wrote {path}");
    Ok(())
}
