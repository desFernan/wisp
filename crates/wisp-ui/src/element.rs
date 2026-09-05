//! What a frame is described with.
//!
//! A tree of boxes, each with a flexbox style and something to paint. It is
//! rebuilt every frame -- there is no retained widget graph to keep in step
//! with the application's own state, which is the bug every retained toolkit
//! spends its life on.
//!
//! Identity is opt-in. A box with an [`Id`] can be asked about afterwards --
//! was it hovered, was it clicked -- and a box without one costs nothing to
//! leave anonymous. Most boxes are anonymous.

use wisp_core::{Rgba, Shadow};

use crate::theme::{Elevation, Role};

/// A name for a box, so that input can be asked about it by name.
///
/// A borrowed string rather than an owned one: these are almost always literals
/// in the source, and a frame should not allocate to name its own buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
}

/// How a box is sized along one dimension.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sizing {
    /// As large as its contents need.
    Hug,
    /// A fixed number of points.
    Fixed(f32),
    /// All of what is left over.
    Fill,
}

/// Where children sit along an axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Start,
    Centre,
    End,
    /// Space between, first at the start and last at the end.
    Between,
    /// Fill the axis: every child stretched to the cross size.
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    pub fn all(v: f32) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }

    pub fn axes(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            bottom: vertical,
            left: horizontal,
            right: horizontal,
        }
    }
}

/// Everything about a box that is not its children.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub axis: Axis,
    pub gap: f32,
    pub padding: Edges,
    pub width: Sizing,
    pub height: Sizing,
    /// Along the axis.
    pub main: Place,
    /// Across it.
    pub cross: Place,
    /// How much of the leftover space this box takes, relative to its
    /// siblings. Zero means none.
    pub grow: f32,
    pub background: Option<Rgba>,
    pub corners: f32,
    pub border: Option<(f32, Rgba)>,
    pub shadow: Option<Shadow>,
    /// Minimum size along each axis, in points.
    pub min: (f32, f32),
    /// Whether this box scrolls its contents rather than growing to fit them.
    pub scroll: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            axis: Axis::Column,
            gap: 0.0,
            padding: Edges::default(),
            width: Sizing::Hug,
            height: Sizing::Hug,
            main: Place::Start,
            cross: Place::Start,
            grow: 0.0,
            background: None,
            corners: 0.0,
            border: None,
            shadow: None,
            min: (0.0, 0.0),
            scroll: false,
        }
    }
}

/// Text inside a box.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub text: String,
    pub role: Role,
    pub colour: Rgba,
    /// Where the line sits in the width it was given.
    pub align: wisp_text::Align,
}

/// One box.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub(crate) id: Option<Id>,
    pub(crate) style: Style,
    pub(crate) label: Option<Label>,
    pub(crate) picture: Option<(crate::pictures::Picture, Rgba)>,
    /// Radians clockwise and the pivot, as a fraction of the box.
    pub(crate) turn: Option<(f32, (f32, f32))>,
    pub(crate) children: Vec<Element>,
}

/// An empty box, laid out as a column.
pub fn div() -> Element {
    Element {
        id: None,
        style: Style::default(),
        label: None,
        picture: None,
        turn: None,
        children: Vec::new(),
    }
}

/// A picture, at its own size unless told otherwise.
///
/// Untinted, which for a picture means white: the tint is multiplied in, so
/// white leaves the colours alone and anything else stains them.
pub fn picture(picture: crate::pictures::Picture) -> Element {
    let mut element = div();
    element.picture = Some((picture, Rgba::hex(0xffffff)));
    element
}

/// A box laid out left to right, with its children centred across it.
///
/// Centred because a row of things of different heights -- a label beside a
/// button, an icon beside a word -- almost always wants them on one line, and
/// a row that does not do this is the single most common thing to have to fix
/// afterwards.
pub fn row() -> Element {
    div().axis(Axis::Row).cross(Place::Centre)
}

/// A box laid out top to bottom.
pub fn column() -> Element {
    div().axis(Axis::Column)
}

/// A piece of text.
pub fn text(content: impl Into<String>, role: Role, colour: Rgba) -> Element {
    let mut element = div();
    element.label = Some(Label {
        text: content.into(),
        role,
        colour,
        align: wisp_text::Align::Start,
    });
    element
}

/// A box that eats whatever space is left, for pushing things apart.
pub fn spacer() -> Element {
    div().grow(1.0)
}

impl Element {
    pub fn id(mut self, id: &'static str) -> Self {
        self.id = Some(Id(id));
        self
    }

    pub fn axis(mut self, axis: Axis) -> Self {
        self.style.axis = axis;
        self
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.style.gap = gap;
        self
    }

    pub fn padding(mut self, padding: Edges) -> Self {
        self.style.padding = padding;
        self
    }

    pub fn pad(self, all: f32) -> Self {
        self.padding(Edges::all(all))
    }

    pub fn width(mut self, width: Sizing) -> Self {
        self.style.width = width;
        self
    }

    pub fn height(mut self, height: Sizing) -> Self {
        self.style.height = height;
        self
    }

    pub fn size(self, width: Sizing, height: Sizing) -> Self {
        self.width(width).height(height)
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.style.min = (width, height);
        self
    }

    pub fn main(mut self, place: Place) -> Self {
        self.style.main = place;
        self
    }

    pub fn cross(mut self, place: Place) -> Self {
        self.style.cross = place;
        self
    }

    /// Centred both ways.
    pub fn centre(self) -> Self {
        self.main(Place::Centre).cross(Place::Centre)
    }

    pub fn grow(mut self, grow: f32) -> Self {
        self.style.grow = grow;
        self
    }

    pub fn background(mut self, colour: Rgba) -> Self {
        self.style.background = Some(colour);
        self
    }

    /// The background for a surface at this elevation, and the shadow that
    /// goes with it.
    ///
    /// Only [`Elevation::Floating`] casts one. Two things in a window both
    /// claiming to be in front is the same as neither of them being.
    pub fn surface(mut self, theme: &crate::theme::Theme, at: Elevation) -> Self {
        self.style.background = Some(theme.surface(at));
        if at == Elevation::Floating {
            self.style.shadow = Some(Shadow {
                offset: (wisp_core::DevicePixels::ZERO, wisp_core::DevicePixels(6.0)),
                blur: wisp_core::DevicePixels(28.0),
                spread: wisp_core::DevicePixels::ZERO,
                colour: Rgba::hex(0x000000).with_alpha(0.45),
            });
        }
        self
    }

    pub fn corners(mut self, radius: f32) -> Self {
        self.style.corners = radius;
        self
    }

    pub fn border(mut self, width: f32, colour: Rgba) -> Self {
        self.style.border = Some((width, colour));
        self
    }

    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.style.shadow = Some(shadow);
        self
    }

    /// Keeps its own size and scrolls what is inside it.
    ///
    /// Named, because where it has been scrolled to has to survive the frame
    /// being rebuilt -- and everything else in this tree is thrown away every
    /// frame on purpose.
    pub fn scroll(mut self, id: &'static str) -> Self {
        self.style.scroll = true;
        self.id = Some(Id(id));
        self
    }

    /// Where this text sits in the width it was given.
    ///
    /// Only meaningful on text, and only visible when the box is wider than
    /// the line -- which for a paragraph in a column it always is, since a
    /// paragraph takes the column's width.
    pub fn align(mut self, align: wisp_text::Align) -> Self {
        if let Some(label) = self.label.as_mut() {
            label.align = align;
        }
        self
    }

    /// Multiplied into a picture. One white icon serves every colour it is
    /// needed in.
    pub fn tint(mut self, colour: Rgba) -> Self {
        if let Some((_, tint)) = self.picture.as_mut() {
            *tint = colour;
        }
        self
    }

    /// Turns a picture clockwise about a point given as a fraction of its own
    /// box: `(0.5, 1.0)` is the bottom edge.
    ///
    /// Only the drawing turns. Layout, hit testing and the click-through mask
    /// all use the box it turned from, which is what keeps a hit area still
    /// while something leans.
    pub fn turn(mut self, radians: f32, pivot: (f32, f32)) -> Self {
        self.turn = Some((radians, pivot));
        self
    }

    pub fn child(mut self, child: Element) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = Element>) -> Self {
        self.children.extend(children);
        self
    }

    /// Adds a child only when `condition` holds.
    ///
    /// Here because the alternative at every call site is an `if` around a
    /// builder chain, which means naming the half-built thing.
    pub fn when(self, condition: bool, child: impl FnOnce() -> Element) -> Self {
        if condition { self.child(child()) } else { self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colour() -> Rgba {
        Rgba::hex(0xffffff)
    }

    #[test]
    fn a_row_centres_its_children_across_itself() {
        // The default that is right almost every time, and the thing most
        // often fixed afterwards in toolkits that do not do it.
        assert_eq!(row().style.cross, Place::Centre);
        assert_eq!(row().style.axis, Axis::Row);
    }

    #[test]
    fn a_column_leaves_them_where_they_are() {
        // Stretching or centring a column's children across it is a decision,
        // not a default: a paragraph centred inside a card is rarely wanted
        // and always surprising.
        assert_eq!(column().style.cross, Place::Start);
    }

    #[test]
    fn only_the_floating_surface_casts_a_shadow() {
        let theme = crate::theme::Theme::dark();
        for at in [Elevation::Sunk, Elevation::Base, Elevation::Raised] {
            assert!(div().surface(&theme, at).style.shadow.is_none(), "{at:?}");
        }
        assert!(
            div()
                .surface(&theme, Elevation::Floating)
                .style
                .shadow
                .is_some()
        );
    }

    #[test]
    fn when_adds_nothing_if_the_condition_is_false() {
        let with = div().when(true, || text("x", Role::Body, colour()));
        let without = div().when(false, || text("x", Role::Body, colour()));
        assert_eq!(with.children.len(), 1);
        assert_eq!(without.children.len(), 0);
    }

    #[test]
    fn a_box_is_anonymous_until_it_is_named() {
        assert_eq!(div().id, None);
        assert_eq!(div().id("send").id, Some(Id("send")));
    }
}
