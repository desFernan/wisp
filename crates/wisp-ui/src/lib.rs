//! Layout, input and a design system.
//!
//! `wisp-core` says what to draw and `wisp-gpu` draws it. This is the part
//! that works out *where*, decides what the pointer just did to it, and ships
//! the type scale and the surface ramp that most toolkits leave to the
//! application and most applications never get round to.

pub mod editor;
pub mod element;
pub mod input;
pub mod pictures;
pub mod theme;
pub mod ui;

pub use editor::{Editor, Preedit};
pub use element::{
    Axis, Edges, Element, Id, Place, Sizing, column, div, picture, row, spacer, text,
};
pub use input::{Composition, Input, Key, Press};
pub use pictures::{Picture, Pictures};
pub use theme::{Elevation, Role, Theme};
pub use ui::{Interactions, Pointer, Ui};
pub use wisp_text::Align;
