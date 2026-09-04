//! Layout, input and a design system.
//!
//! `wisp-core` says what to draw and `wisp-gpu` draws it. This is the part
//! that works out *where*, decides what the pointer just did to it, and ships
//! the type scale and the surface ramp that most toolkits leave to the
//! application and most applications never get round to.

pub mod element;
pub mod theme;
pub mod ui;

pub use element::{Axis, Edges, Element, Id, Place, Sizing, column, div, row, spacer, text};
pub use theme::{Elevation, Role, Theme};
pub use ui::{Interactions, Pointer, Ui};
