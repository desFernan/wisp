//! Renders a frame to a PNG with no window.
//!
//!   cargo run --example shot -- out.png
//!
//! The same layout, renderer and fonts a window would use. It exists because
//! looking at a window means a machine with a screen and a window nothing is
//! covering, and neither is true often enough to rely on.

use wisp::{Editor, OnEnter, Rgba, Role, Theme, Ui};

#[path = "chat.rs"]
mod chat;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "wisp.png".into());
    let theme = Theme::dark();
    let mut editor = Editor::new("한글도 조합해서 들어갑니다");
    wisp::snapshot::write(&path, (1040.0, 680.0), 2.0, theme.base, |ui: &mut Ui| {
        ui.focus("composer");
        let (line, _) = ui.field(
            "composer",
            &mut editor,
            &theme,
            Role::Body,
            "Say something to Puck…",
            OnEnter::Submit,
        );
        chat::window(&theme, ui, &chat::opening(), line)
    })?;
    println!("wrote {path}");
    Ok(())
}
