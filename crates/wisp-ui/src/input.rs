//! What the keyboard and the input method said, in terms the toolkit uses.
//!
//! Deliberately not winit's types. A key press here is what it *means* --
//! "move a word left", "select to the end" -- rather than which physical key
//! was struck with which modifiers, so that the platform layer decides once
//! what alt+left means on this operating system and nothing above it has to
//! know.

/// One thing the keyboard asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Text that was typed and committed. From a plain key, or from an input
    /// method finishing a syllable.
    Insert(String),
    Backspace,
    Delete,
    Left,
    Right,
    /// The start and end of the line.
    Home,
    End,
    Enter,
    Escape,
    Tab,
    SelectAll,
    Copy,
    Cut,
    Paste,
}

/// A key press and the modifiers that change what it means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Press {
    pub key: Key,
    /// Extends the selection rather than moving the caret.
    pub shift: bool,
    /// Moves by words rather than by characters. Alt on macOS.
    pub word: bool,
    /// Enter with this held is a newline rather than a send, and the other way
    /// round, depending on what the field was asked for.
    pub modifier: bool,
}

impl Press {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            shift: false,
            word: false,
            modifier: false,
        }
    }
}

/// Something the input method is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Composition {
    /// Being composed, and the input method's caret inside it.
    Preedit(String, Option<(usize, usize)>),
    /// Finished; this text belongs to the document now.
    Commit(String),
}

/// Anything that arrives between frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Key(Press),
    Ime(Composition),
}
