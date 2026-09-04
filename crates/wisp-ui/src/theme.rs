//! A type scale, a surface ramp and a palette, shipped rather than left to the
//! application.
//!
//! Most toolkits hand you `text_size(f32)` and a colour type and call the rest
//! taste. What actually happens is that every size and every grey is chosen one
//! at a time, each reasonable beside the thing next to it and none of them
//! related to the rest -- and the result is flat, because nothing is bigger or
//! brighter than anything else by enough to notice.
//!
//! This was measured rather than assumed. The application wisp was extracted
//! from had twenty-seven pieces of text in three sizes, twenty of them the
//! smallest, and four surfaces about six percent apart -- under what the eye
//! separates on a dark screen. It read as one black rectangle however
//! carefully the things on it were arranged.
//!
//! So the steps have names. A caller asks for [`Role::Title`] rather than for
//! nineteen points, and the question at each site becomes *what is this*
//! instead of *how big should this be*.

use wisp_core::{DevicePixels, Rgba, Scale};

/// What a piece of text is for. Size and weight follow from that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The one thing a screen is about. At most one per screen: a second
    /// display-sized thing means neither of them is the answer.
    Display,
    Title,
    /// What is read. Replies, messages, the rows of a list.
    Body,
    /// The name of a control -- shorter than a sentence.
    Label,
    /// Metadata only. Timestamps, counts, status. Anything a reader can skip
    /// without losing the thread.
    Caption,
}

impl Role {
    pub const EVERY: [Role; 5] = [
        Role::Display,
        Role::Title,
        Role::Body,
        Role::Label,
        Role::Caption,
    ];

    /// In points. Neighbouring steps are about a fifth apart at the bottom and
    /// further at the top: close enough that the set looks deliberate, far
    /// enough that two of them are never mistaken for each other.
    pub fn size(self) -> f32 {
        match self {
            Self::Display => 26.0,
            Self::Title => 17.0,
            Self::Body => 14.0,
            Self::Label => 12.5,
            Self::Caption => 11.0,
        }
    }

    /// 400, 500, 700 -- the three weights a text stack can be relied on to
    /// have. Asking for 600 on a family that ships five weights gets 700
    /// anyway, and asking for it on one that ships two gets 400.
    pub fn weight(self) -> wisp_text::Weight {
        match self {
            Self::Display => wisp_text::Weight::Bold,
            Self::Title => wisp_text::Weight::Medium,
            Self::Body | Self::Caption => wisp_text::Weight::Regular,
            Self::Label => wisp_text::Weight::Medium,
        }
    }

    /// A multiple of the size. Tighter as text grows: the leading that makes a
    /// paragraph readable makes a heading look like it has come apart.
    pub fn leading(self) -> f32 {
        match self {
            Self::Display => 1.2,
            Self::Title => 1.3,
            Self::Body => 1.55,
            Self::Label | Self::Caption => 1.4,
        }
    }

    /// The font this role asks for, at a given display scale.
    pub fn font(self, scale: Scale) -> wisp_text::Font {
        let size = DevicePixels(self.size() * scale.factor());
        wisp_text::Font {
            family: None,
            weight: self.weight(),
            size,
            line_height: size * self.leading(),
        }
    }
}

/// How far a surface is lifted off the window's background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elevation {
    /// Below the page: a sidebar, a status line.
    Sunk,
    /// The page.
    Base,
    /// A card on the page.
    Raised,
    /// Over the page: a popover, a composer, anything you are meant to reach
    /// for. The only step that carries a shadow.
    Floating,
}

impl Elevation {
    pub const EVERY: [Elevation; 4] = [
        Elevation::Sunk,
        Elevation::Base,
        Elevation::Raised,
        Elevation::Floating,
    ];
}

/// A whole palette, dark or light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub sunk: Rgba,
    pub base: Rgba,
    pub raised: Rgba,
    pub floating: Rgba,
    pub border: Rgba,
    /// What text is set in.
    pub ink: Rgba,
    /// Text that is deliberately quieter than the rest.
    pub quiet: Rgba,
    /// One colour, used sparingly. A screen with three accents has none.
    pub accent: Rgba,
    /// What reads well on top of the accent.
    pub on_accent: Rgba,
    pub success: Rgba,
    pub danger: Rgba,
    pub warning: Rgba,
}

impl Theme {
    /// The dark theme, and the default: a toolkit whose headline is a window
    /// over somebody's desktop should not be the brightest thing on it.
    pub fn dark() -> Self {
        Self {
            sunk: Rgba::hex(0x0d0d11),
            base: Rgba::hex(0x16161c),
            raised: Rgba::hex(0x21212a),
            floating: Rgba::hex(0x2b2b36),
            border: Rgba::hex(0x393947),
            ink: Rgba::hex(0xe8e8f0),
            quiet: Rgba::hex(0x9a9aa8),
            accent: Rgba::hex(0xed8c33),
            on_accent: Rgba::hex(0x1a1208),
            success: Rgba::hex(0x3fb950),
            danger: Rgba::hex(0xf85149),
            warning: Rgba::hex(0xe3b341),
        }
    }

    pub fn light() -> Self {
        Self {
            // A light theme runs out of headroom at white, so the steps below
            // it have to leave room rather than all of them being white and
            // relying on the shadow to tell them apart.
            sunk: Rgba::hex(0xe9e9ef),
            base: Rgba::hex(0xf4f4f8),
            raised: Rgba::hex(0xfbfbfd),
            floating: Rgba::hex(0xffffff),
            border: Rgba::hex(0xd6d6de),
            ink: Rgba::hex(0x1b1b22),
            quiet: Rgba::hex(0x6b6b78),
            accent: Rgba::hex(0xc96a13),
            on_accent: Rgba::hex(0xfff6ec),
            success: Rgba::hex(0x1a7f37),
            danger: Rgba::hex(0xcf222e),
            warning: Rgba::hex(0x9a6700),
        }
    }

    pub fn surface(&self, at: Elevation) -> Rgba {
        match at {
            Elevation::Sunk => self.sunk,
            Elevation::Base => self.base,
            Elevation::Raised => self.raised,
            Elevation::Floating => self.floating,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_scale_only_goes_down() {
        // Two roles the same size are two names for one thing, and which of
        // them gets used becomes a coin toss.
        for pair in Role::EVERY.windows(2) {
            assert!(
                pair[0].size() > pair[1].size(),
                "{:?} is not larger than {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn no_two_steps_are_close_enough_to_look_like_an_accident() {
        for pair in Role::EVERY.windows(2) {
            let ratio = pair[0].size() / pair[1].size();
            assert!(
                ratio > 1.08,
                "{:?} and {:?} are only {ratio:.3} apart",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn nothing_is_too_small_to_read() {
        for role in Role::EVERY {
            assert!(role.size() >= 11.0, "{role:?} is {}pt", role.size());
        }
    }

    #[test]
    fn a_heading_is_set_tighter_than_a_paragraph() {
        assert!(Role::Body.leading() > Role::Title.leading());
        assert!(Role::Title.leading() > Role::Display.leading());
    }

    fn luma(c: Rgba) -> f32 {
        // Rough and good enough to compare two greys with.
        0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
    }

    #[test]
    fn the_surfaces_are_far_enough_apart_to_be_seen() {
        for theme in [Theme::dark(), Theme::light()] {
            for pair in Elevation::EVERY.windows(2) {
                let lift = (luma(theme.surface(pair[1])) - luma(theme.surface(pair[0]))).abs();
                assert!(
                    lift > 0.008,
                    "{:?} to {:?} is only {lift:.4}",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn text_can_be_read_on_every_surface_it_is_set_on() {
        // Not a full contrast calculation -- a floor, so that a palette cannot
        // be committed with grey text on a grey card.
        for theme in [Theme::dark(), Theme::light()] {
            for at in Elevation::EVERY {
                let surface = luma(theme.surface(at));
                for (name, text) in [("ink", theme.ink), ("quiet", theme.quiet)] {
                    let gap = (luma(text) - surface).abs();
                    assert!(gap > 0.15, "{name} on {at:?} differs by only {gap:.3}");
                }
            }
        }
    }

    #[test]
    fn the_accent_is_legible_against_what_is_written_on_it() {
        for theme in [Theme::dark(), Theme::light()] {
            let gap = (luma(theme.accent) - luma(theme.on_accent)).abs();
            assert!(gap > 0.25, "accent and on_accent differ by only {gap:.3}");
        }
    }
}
