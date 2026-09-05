//! Laying a frame out, painting it, and answering questions about what the
//! pointer did to it.
//!
//! The loop is: build an [`Element`] tree, hand it here, get a [`Scene`] and a
//! set of answers. Nothing is retained between frames except the pointer's
//! state, which belongs to the mouse rather than to the interface.

use std::collections::{HashMap, HashSet};

use taffy::prelude::*;
use wisp_core::geometry::{Point, Rect};
use wisp_core::scene::{Background, Border, Corners, Quad};
use wisp_core::{DevicePixels, Scale, Scene};
use wisp_text::TextSystem;

use crate::editor::Editor;
use crate::element::{Axis, Edges, Element, Id, Label, Place, Sizing, div, row, text};
use crate::input::{Composition, Input, Key};
use crate::theme::{Role, Theme};

/// What the pointer is doing, in points.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pointer {
    pub at: (f32, f32),
    pub down: bool,
}

/// What happened to the named boxes in the frame just laid out.
#[derive(Debug, Default, Clone)]
pub struct Interactions {
    hovered: Option<Id>,
    pressed: Option<Id>,
    clicked: Option<Id>,
    boxes: HashMap<Id, Rect<f32>>,
}

impl Interactions {
    pub fn hovered(&self, id: &'static str) -> bool {
        self.hovered == Some(Id(id))
    }

    /// Held down on this box.
    pub fn pressed(&self, id: &'static str) -> bool {
        self.pressed == Some(Id(id))
    }

    /// Pressed and released on the same box, which is what a click is.
    ///
    /// Not "released over it": pressing one button, sliding onto another and
    /// letting go there should do nothing, and does.
    pub fn clicked(&self, id: &'static str) -> bool {
        self.clicked == Some(Id(id))
    }

    /// What was clicked, whatever it was called.
    pub fn clicked_id(&self) -> Option<Id> {
        self.clicked
    }

    /// Where a named box ended up, in points.
    pub fn bounds(&self, id: &'static str) -> Option<Rect<f32>> {
        self.boxes.get(&Id(id)).copied()
    }

    /// The same, for an [`Id`] that is already in hand.
    pub fn bounds_of(&self, id: Id) -> Option<Rect<f32>> {
        self.boxes.get(&id).copied()
    }
}

/// Everything that outlives a frame: fonts, and what the mouse was doing last
/// time.
pub struct Ui {
    text: TextSystem,
    pictures: crate::pictures::Pictures,
    pointer: Pointer,
    was_down: bool,
    pressed_on: Option<Id>,
    last: Interactions,
    /// Which field the keyboard is talking to, if any.
    focused: Option<Id>,
    /// Everything that arrived since the last frame, waiting for the field it
    /// belongs to to be built.
    pending: Vec<Input>,
    /// The fields that existed in the frame just gone. Clicking anything that
    /// is not one of these puts the keyboard down.
    fields: HashSet<Id>,
    seen: HashSet<Id>,
    /// How far each scrolling box has been scrolled, in points.
    scrolls: HashMap<Id, f32>,
    /// Wheel movement since the last frame, in points.
    wheel: f32,
    /// The display scale of the frame being built, so that a picture can be
    /// measured at its own pixels while everything else is in points.
    scale: f32,
    /// Whether a click finished this frame, wherever it landed.
    ///
    /// Not the same question as which box was clicked: most of a window is
    /// unnamed, and clicking the empty part of it should still put the
    /// keyboard down.
    released: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    pub fn new() -> Self {
        Self {
            text: TextSystem::new(),
            pictures: crate::pictures::Pictures::default(),
            pointer: Pointer::default(),
            was_down: false,
            pressed_on: None,
            last: Interactions::default(),
            focused: None,
            pending: Vec::new(),
            fields: HashSet::new(),
            seen: HashSet::new(),
            scrolls: HashMap::new(),
            wheel: 0.0,
            released: false,
            scale: 1.0,
        }
    }

    pub fn text(&mut self) -> &mut TextSystem {
        &mut self.text
    }

    /// Everything drawn from pixels: avatars, icons, sprites.
    pub fn pictures(&mut self) -> &mut crate::pictures::Pictures {
        &mut self.pictures
    }

    /// What the pointer did to the frame before this one.
    ///
    /// The whole of how a button works here. The tree is rebuilt every frame
    /// and nothing in it remembers being pressed, so the answer to "is this
    /// button held down" is a question about the frame that has already been
    /// drawn -- which is one frame of lag on a highlight, and no widget state
    /// to keep in step with anything.
    pub fn last(&self) -> &Interactions {
        &self.last
    }

    /// Where the pointer is and whether its button is down, in points.
    pub fn point(&mut self, pointer: Pointer) {
        self.pointer = pointer;
    }

    /// The wheel turned, in points. Positive scrolls the content up, the way
    /// every list on this platform does.
    pub fn wheel(&mut self, by: f32) {
        self.wheel += by;
    }

    /// Something the keyboard or the input method did.
    ///
    /// Queued rather than applied: which field it belongs to is not known
    /// until that field is built, and the frame that builds it has not run
    /// yet.
    pub fn input(&mut self, input: Input) {
        self.pending.push(input);
    }

    /// Which field the keyboard is talking to.
    pub fn focused(&self) -> Option<Id> {
        self.focused
    }

    pub fn focus(&mut self, id: &'static str) {
        self.focused = Some(Id(id));
    }

    pub fn blur(&mut self) {
        self.focused = None;
        // Anything half composed goes with the focus. Leaving it queued means
        // it lands in whatever is focused next, which is somebody else's
        // half-typed syllable appearing in their password field.
        self.pending.retain(|input| !matches!(input, Input::Ime(_)));
    }

    /// Whether this field has the keyboard.
    pub fn has_focus(&self, id: &'static str) -> bool {
        self.focused == Some(Id(id))
    }

    /// Lays out `root` to fill `size` points, paints it into `scene`, and
    /// reports what the pointer did.
    pub fn frame(
        &mut self,
        root: &Element,
        size: (f32, f32),
        scale: Scale,
        scene: &mut Scene,
    ) -> Interactions {
        self.scale = scale.factor();
        let mut tree: TaffyTree<Label> = TaffyTree::new();
        // Nothing here is rounded to whole points. This is a library whose
        // argument is that positions are fractional, and a layout engine
        // snapping every box to a pixel would take that away one box at a
        // time -- starting with a label rounded a third of a point narrower
        // than its own text, which wraps the last letter onto its own line.
        tree.disable_rounding();
        // The root has no parent, and fills the window either way.
        let node = self.build(&mut tree, root, Axis::Column, false);
        // Pinned to the window rather than left to ask for a share of a parent
        // it does not have. Without this a root that says it fills the window
        // is laid out at the height of its own contents and everything below
        // them is empty.
        if let Ok(mut style) = tree.style(node).cloned() {
            // Only the axes that asked to fill. A root given a fixed size
            // still gets it, which is what a test laying out one box in a
            // notional window is doing.
            if matches!(root.style.width, Sizing::Fill) {
                style.size.width = Dimension::length(size.0);
            }
            if matches!(root.style.height, Sizing::Fill) {
                style.size.height = Dimension::length(size.1);
            }
            let _ = tree.set_style(node, style);
        }
        let space = taffy::Size {
            width: AvailableSpace::Definite(size.0),
            height: AvailableSpace::Definite(size.1),
        };
        // Text is measured by taffy rather than before it, because how wide a
        // paragraph is depends on how much room it was given. Sizing it to its
        // natural width first and letting flexbox sort it out is how a long
        // line ends up running off the side of the window: nothing downstream
        // can make a leaf narrower than it said it was.
        let text = &mut self.text;
        let measured =
            tree.compute_layout_with_measure(node, space, |input, _id, label, _style| {
                let known = input.known_dimensions;
                let Some(label) = label else {
                    // A leaf with nothing in it is whatever it was told to be.
                    // Answering "nothing" here is not the same as declining to
                    // answer: taffy asks every leaf, so an empty box would
                    // come back zero by zero however it was styled.
                    return taffy::tree::LayoutOutput::from_outer_size(taffy::Size {
                        width: known.width.unwrap_or(0.0),
                        height: known.height.unwrap_or(0.0),
                    });
                };
                let wrap = known.width.or(match input.available_space.width {
                    AvailableSpace::Definite(width) => Some(width),
                    // Asked how small or how large it could possibly be, a
                    // paragraph answers unwrapped. The pass that hands it a
                    // real width is the one that decides.
                    _ => None,
                });
                let font = label.role.font(scale);
                let (w, h) = text.measure_aligned(
                    &label.text,
                    &font,
                    wrap.map(|w| DevicePixels(w * scale.factor())),
                    label.align,
                );
                taffy::tree::LayoutOutput::from_outer_size(taffy::Size {
                    width: known.width.unwrap_or(w.get() / scale.factor()),
                    height: known.height.unwrap_or(h.get() / scale.factor()),
                })
            });
        if measured.is_err() {
            self.last = Interactions::default();
            // A layout that will not solve is a bug in the tree, not something
            // to take the frame down for. An empty scene is visible and
            // reportable; a panic in a render loop is neither.
            return Interactions::default();
        }

        let mut painted = Vec::new();
        self.apply_wheel(&tree, node, root, (0.0, 0.0));
        self.paint(
            &tree,
            node,
            root,
            (0.0, 0.0),
            scale,
            scene,
            &mut painted,
            None,
        );
        self.last = self.resolve(painted);

        // Clicking anything that is not a field puts the keyboard down. The
        // set is this frame's fields, and the click is this frame's click, so
        // a click *on* a field never blurs it -- the field picks the same click
        // up next frame and takes the keyboard.
        self.fields = std::mem::take(&mut self.seen);
        let onto_a_field = self
            .last
            .clicked
            .is_some_and(|id| self.fields.contains(&id));
        if self.released && !onto_a_field {
            self.blur();
        }
        // Keystrokes nobody was listening for. Kept, they would arrive in
        // whatever is focused next, which is somebody's half-typed sentence
        // appearing in the field they just clicked into.
        self.pending.clear();
        Interactions {
            hovered: self.last.hovered,
            pressed: self.last.pressed,
            clicked: self.last.clicked,
            boxes: self.last.boxes.clone(),
        }
    }

    /// Mirrors the element tree into taffy, measuring text as it goes.
    fn build(
        &mut self,
        tree: &mut TaffyTree<Label>,
        element: &Element,
        along: Axis,
        inside_scroll: bool,
    ) -> NodeId {
        let style = to_taffy(element, along, inside_scroll);
        if let Some(label) = element.label.as_ref() {
            return tree
                .new_leaf_with_context(style, label.clone())
                .expect("a leaf is always makeable");
        }
        if let Some((picture, _)) = element.picture {
            // Its own size unless the caller said otherwise. Measured here
            // rather than through the callback: a picture's size is known and
            // does not depend on how much room it is given.
            let mut style = style;
            if matches!(element.style.width, Sizing::Hug) {
                style.size.width = Dimension::length(picture.size.0 as f32 / self.scale);
            }
            if matches!(element.style.height, Sizing::Hug) {
                style.size.height = Dimension::length(picture.size.1 as f32 / self.scale);
            }
            return tree.new_leaf(style).expect("a leaf is always makeable");
        }
        let children: Vec<NodeId> = element
            .children
            .iter()
            .map(|child| self.build(tree, child, element.style.axis, element.style.scroll))
            .collect();
        tree.new_with_children(style, &children)
            .expect("a node with children is always makeable")
    }

    /// Walks the laid-out tree, emitting quads and glyphs and collecting the
    /// named boxes.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &mut self,
        tree: &TaffyTree<Label>,
        node: NodeId,
        element: &Element,
        offset: (f32, f32),
        scale: Scale,
        scene: &mut Scene,
        painted: &mut Vec<(Id, Rect<f32>)>,
        clip: Option<Rect<f32>>,
    ) {
        let layout = tree.layout(node).expect("laid out above");
        let at = (offset.0 + layout.location.x, offset.1 + layout.location.y);
        let bounds = Rect::from_edges(
            at.0,
            at.1,
            at.0 + layout.size.width,
            at.1 + layout.size.height,
        );

        if let Some(id) = element.id {
            painted.push((id, bounds));
        }

        // A scrolling box cuts everything inside it at its own edges, and
        // inside a box that is already cut, at the overlap -- otherwise a list
        // inside a panel spills past the panel the moment the panel scrolls.
        let (clip, child_offset) = if element.style.scroll {
            let inner = match clip {
                Some(outer) => outer.intersection(&bounds).unwrap_or(Rect::default()),
                None => bounds,
            };
            let scrolled = element
                .id
                .and_then(|id| self.scrolls.get(&id).copied())
                .unwrap_or(0.0);
            (Some(inner), (at.0, at.1 - scrolled))
        } else {
            (clip, at)
        };
        let device_clip = clip.map(|c| {
            Rect::from_edges(
                DevicePixels(c.left() * scale.factor()),
                DevicePixels(c.top() * scale.factor()),
                DevicePixels(c.right() * scale.factor()),
                DevicePixels(c.bottom() * scale.factor()),
            )
        });

        let style = &element.style;
        if style.background.is_some() || style.border.is_some() || style.shadow.is_some() {
            let device = Rect::from_edges(
                DevicePixels(bounds.left() * scale.factor()),
                DevicePixels(bounds.top() * scale.factor()),
                DevicePixels(bounds.right() * scale.factor()),
                DevicePixels(bounds.bottom() * scale.factor()),
            );
            let mut quad = Quad::new(
                device,
                Background::Solid(style.background.unwrap_or(wisp_core::Rgba::TRANSPARENT)),
            )
            .with_corners(Corners::all(DevicePixels(style.corners * scale.factor())));
            if let Some((width, colour)) = style.border {
                quad = quad.with_border(Border {
                    width: DevicePixels(width * scale.factor()),
                    colour,
                });
            }
            if let Some(shadow) = style.shadow {
                quad = quad.with_shadow(shadow);
            }
            scene.push(quad.clipped_to(device_clip));
        }

        if let Some((picture, tint)) = element.picture {
            scene.push_textured(
                wisp_core::scene::Textured::new(
                    Rect::from_edges(
                        DevicePixels(bounds.left() * scale.factor()),
                        DevicePixels(bounds.top() * scale.factor()),
                        DevicePixels(bounds.right() * scale.factor()),
                        DevicePixels(bounds.bottom() * scale.factor()),
                    ),
                    picture.uv,
                )
                .tinted(tint)
                .clipped_to(device_clip)
                .turned(
                    element.turn.map(|(r, _)| r).unwrap_or(0.0),
                    element.turn.map(|(_, p)| p).unwrap_or((0.5, 0.5)),
                ),
            );
        }

        if let Some(label) = element.label.as_ref() {
            let font = label.role.font(scale);
            self.text.draw_all(
                scene,
                &label.text,
                &font,
                Point::new(
                    DevicePixels(bounds.left() * scale.factor()),
                    DevicePixels(bounds.top() * scale.factor()),
                ),
                // Wrapped at the width it was actually given, which is not
                // always the width it asked for. Half a point of slack: the
                // measurement and the layout are separate floating point
                // journeys to the same number, and a label that comes back a
                // ten-thousandth narrower than it measured drops its last
                // letter onto a second line.
                Some(DevicePixels((bounds.size.width + 0.5) * scale.factor())),
                label.colour,
                device_clip,
                label.align,
            );
        }

        let children = tree.children(node).unwrap_or_default();
        for (child_node, child) in children.into_iter().zip(element.children.iter()) {
            self.paint(
                tree,
                child_node,
                child,
                child_offset,
                scale,
                scene,
                painted,
                clip,
            );
        }
    }

    /// Gives the wheel to the innermost scrolling box under the pointer.
    ///
    /// Innermost, because a list inside a panel inside a window is three boxes
    /// that could all take it, and the one you are pointing at is the one you
    /// meant. Walking the tree finds it in painting order, and the last match
    /// is the deepest.
    fn apply_wheel(
        &mut self,
        tree: &TaffyTree<Label>,
        node: NodeId,
        element: &Element,
        offset: (f32, f32),
    ) {
        if self.wheel == 0.0 {
            return;
        }
        let mut target = None;
        collect_scrollables(tree, node, element, offset, &mut target, self.pointer.at);
        let Some((id, room)) = target else {
            // Nothing under the pointer scrolls. Dropped rather than saved for
            // later: a wheel turn is about where the pointer is now.
            self.wheel = 0.0;
            return;
        };
        let at = self.scrolls.entry(id).or_insert(0.0);
        // Clamped to what there is. Rubber-banding past the end is a platform
        // convention rather than a layout question, and guessing at it here
        // would be wrong on at least one platform.
        *at = (*at - self.wheel).clamp(0.0, room.max(0.0));
        self.wheel = 0.0;
    }

    /// Turns the frame's boxes and the pointer's state into answers.
    fn resolve(&mut self, painted: Vec<(Id, Rect<f32>)>) -> Interactions {
        let point = Point::new(self.pointer.at.0, self.pointer.at.1);
        // Last wins: a box painted later is drawn on top of one painted
        // earlier, and the thing you can see is the thing you are pointing at.
        let hovered = painted
            .iter()
            .rev()
            .find(|(_, bounds)| bounds.contains(point))
            .map(|(id, _)| *id);

        let went_down = self.pointer.down && !self.was_down;
        let came_up = !self.pointer.down && self.was_down;
        if went_down {
            self.pressed_on = hovered;
        }
        let clicked = if came_up && self.pressed_on.is_some() && self.pressed_on == hovered {
            self.pressed_on
        } else {
            None
        };
        self.released = came_up;
        if came_up {
            self.pressed_on = None;
        }
        self.was_down = self.pointer.down;

        Interactions {
            hovered,
            pressed: if self.pointer.down {
                self.pressed_on
            } else {
                None
            },
            clicked,
            boxes: painted.into_iter().collect(),
        }
    }
}

fn place(place: Place) -> Option<AlignItems> {
    match place {
        Place::Start => Some(AlignItems::FLEX_START),
        Place::Centre => Some(AlignItems::CENTER),
        Place::End => Some(AlignItems::FLEX_END),
        Place::Stretch => Some(AlignItems::STRETCH),
        Place::Between => None,
    }
}

fn justify(p: Place) -> Option<JustifyContent> {
    match p {
        Place::Start => Some(JustifyContent::FLEX_START),
        Place::Centre => Some(JustifyContent::CENTER),
        Place::End => Some(JustifyContent::FLEX_END),
        Place::Between => Some(JustifyContent::SPACE_BETWEEN),
        Place::Stretch => Some(JustifyContent::FLEX_START),
    }
}

/// How a [`Sizing`] reads across the parent's axis: as a fraction of it.
fn across(s: Sizing) -> Dimension {
    match s {
        Sizing::Hug => Dimension::auto(),
        Sizing::Fixed(v) => Dimension::length(v),
        Sizing::Fill => Dimension::percent(1.0),
    }
}

/// How it reads along the parent's axis.
///
/// `Fill` is the whole reason this is not one function. Along the axis, "fill"
/// means *the space that is left*, which is flex grow -- not a hundred percent
/// of the parent, which is what a sidebar and a pane both asking for it add up
/// to, and how a pane ends up hanging off the side of the window while the
/// sidebar it was next to is squeezed narrower than it asked to be.
fn along(s: Sizing) -> Dimension {
    match s {
        Sizing::Hug => Dimension::auto(),
        Sizing::Fixed(v) => Dimension::length(v),
        Sizing::Fill => Dimension::auto(),
    }
}

fn to_taffy(element: &Element, parent: Axis, inside_scroll: bool) -> taffy::Style {
    let s = &element.style;
    let (main, cross) = match parent {
        Axis::Row => (s.width, s.height),
        Axis::Column => (s.height, s.width),
    };
    let (width, height) = match parent {
        Axis::Row => (along(main), across(cross)),
        Axis::Column => (across(cross), along(main)),
    };
    // Filling along the axis is grow; a fixed size along it does not give way.
    let grow = if matches!(main, Sizing::Fill) {
        s.grow.max(1.0)
    } else {
        s.grow
    };
    // Inside a scrolling box, nothing gives way. Flexbox's instinct is to
    // squash the contents until they fit, which for a scrolling box is the
    // opposite of the point: there would be nothing to scroll, because
    // everything already fits.
    let shrink = if inside_scroll || matches!(main, Sizing::Fixed(_)) {
        0.0
    } else {
        1.0
    };
    // A paragraph in a column takes the column's width. Left to size itself it
    // measures unwrapped -- one very long line -- so the box around it is built
    // for one line while the paint wraps to three, and the next thing down is
    // drawn over it.
    //
    // In a row it is left alone: a label beside a button is a word, not a
    // paragraph, and stretching it there would push the button off the end.
    let stretch_text = element.label.is_some()
        && parent == Axis::Column
        && matches!(element.style.width, Sizing::Hug);
    taffy::Style {
        display: Display::Flex,
        flex_direction: match s.axis {
            Axis::Row => FlexDirection::Row,
            Axis::Column => FlexDirection::Column,
        },
        gap: taffy::Size {
            width: LengthPercentage::length(s.gap),
            height: LengthPercentage::length(s.gap),
        },
        padding: taffy::Rect {
            top: LengthPercentage::length(s.padding.top),
            right: LengthPercentage::length(s.padding.right),
            bottom: LengthPercentage::length(s.padding.bottom),
            left: LengthPercentage::length(s.padding.left),
        },
        size: taffy::Size { width, height },
        min_size: taffy::Size {
            width: LengthPercentageAuto::length(s.min.0),
            height: LengthPercentageAuto::length(s.min.1),
        },
        // Deliberately not taffy's `Overflow::Scroll`. Turning it on changes
        // how the contents are measured -- a paragraph inside one came back
        // sized for a single unwrapped line, so the box drawn around it was
        // one line tall and the text ran out of the bottom and under the next
        // message. Cutting and offsetting is done here instead, and the
        // clipping was already this library's job.
        //
        // What a scrolling box does need is a definite height, which is what
        // gives its contents somewhere to overflow *from*.
        align_self: stretch_text.then_some(taffy::AlignSelf::STRETCH),
        align_items: place(s.cross),
        justify_content: justify(s.main),
        flex_grow: grow,
        // Allowed to shrink, which is flexbox's own default. Refusing to was
        // an attempt to stop text being crushed, and it was the wrong lever:
        // text has a measure function, so shrinking a paragraph wraps it. What
        // refusing actually did was let a row of children add up to more than
        // the window and run off the side of it.
        flex_shrink: shrink,
        ..Default::default()
    }
}

/// How a field answers the return key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnEnter {
    /// Return submits, and the modifier makes a new line. What a composer in a
    /// chat window wants.
    Submit,
    /// Return always makes a new line. What an editor wants.
    Newline,
}

/// What a field did with the input it was handed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Edited {
    /// The text changed.
    pub changed: bool,
    /// Return was pressed and the field is set to submit on it.
    pub submitted: bool,
    /// Escape was pressed.
    pub cancelled: bool,
}

impl Ui {
    /// An editable line, and the keyboard input that belongs to it.
    ///
    /// The element is built and the pending input is applied in the same call,
    /// because they are the same question: this is the field that has the
    /// keyboard, so this is the field the keystrokes were for.
    pub fn field(
        &mut self,
        id: &'static str,
        editor: &mut Editor,
        theme: &Theme,
        role: Role,
        placeholder: &str,
        on_enter: OnEnter,
    ) -> (Element, Edited) {
        let name = Id(id);
        self.seen.insert(name);
        // Clicking a field takes the keyboard. The click is from the frame
        // before, which is the frame the user was looking at when they aimed.
        if self.last.clicked(id) {
            self.focused = Some(name);
        }

        let mut edited = Edited::default();
        if self.focused == Some(name) {
            for input in std::mem::take(&mut self.pending) {
                edited = self.apply(editor, input, on_enter, edited);
            }
        }

        let focused = self.focused == Some(name);
        let quiet = theme.quiet;
        let mut line = row().gap(0.0).id(id).cross(Place::Centre);

        if editor.text().is_empty() && editor.preedit().text.is_empty() {
            line = line.child(text(placeholder, role, quiet));
            if focused {
                line = line.child(caret(theme, role));
            }
            return (line, edited);
        }

        let content = editor.text().to_string();
        match editor.selection() {
            Some((from, to)) if focused => {
                line = line
                    .child(text(content[..from].to_string(), role, theme.ink))
                    .child(
                        row()
                            // A selection is a run of text with something
                            // behind it, not a rectangle drawn over the top:
                            // over the top would need the glyphs redrawn to
                            // stay legible.
                            .background(theme.accent.with_alpha(0.30))
                            .corners(3.0)
                            .child(text(content[from..to].to_string(), role, theme.ink)),
                    )
                    .child(text(content[to..].to_string(), role, theme.ink));
            }
            _ => {
                let at = editor.caret();
                line = line.child(text(content[..at].to_string(), role, theme.ink));
                if focused && !editor.preedit().text.is_empty() {
                    line = line.child(composing(&editor.preedit().text, theme, role));
                } else if focused {
                    line = line.child(caret(theme, role));
                }
                line = line.child(text(content[at..].to_string(), role, theme.ink));
            }
        }
        (line, edited)
    }

    fn apply(
        &mut self,
        editor: &mut Editor,
        input: Input,
        on_enter: OnEnter,
        mut edited: Edited,
    ) -> Edited {
        match input {
            Input::Ime(Composition::Preedit(text, cursor)) => {
                editor.compose(text, cursor);
                edited.changed = true;
            }
            Input::Ime(Composition::Commit(text)) => {
                editor.insert(&text);
                edited.changed = true;
            }
            Input::Key(press) => match press.key {
                Key::Insert(text) => {
                    editor.insert(&text);
                    edited.changed = true;
                }
                Key::Backspace => {
                    editor.backspace();
                    edited.changed = true;
                }
                Key::Delete => {
                    editor.delete();
                    edited.changed = true;
                }
                Key::Left if press.word => editor.move_word_left(press.shift),
                Key::Right if press.word => editor.move_word_right(press.shift),
                Key::Left => editor.move_left(press.shift),
                Key::Right => editor.move_right(press.shift),
                Key::Home => editor.move_home(press.shift),
                Key::End => editor.move_end(press.shift),
                Key::SelectAll => editor.select_all(),
                Key::Escape => edited.cancelled = true,
                Key::Enter => match (on_enter, press.modifier) {
                    (OnEnter::Submit, false) => edited.submitted = true,
                    _ => {
                        editor.insert("\n");
                        edited.changed = true;
                    }
                },
                // Nothing yet: the clipboard is the platform's, and reaching
                // for it from here would put a platform in this crate.
                Key::Copy | Key::Cut | Key::Paste | Key::Tab => {}
            },
        }
        edited
    }
}

/// The blinking bar. Not blinking yet -- a caret that blinks needs a clock,
/// and a clock in a frame is a redraw every half second whether or not
/// anything changed.
fn caret(theme: &Theme, role: Role) -> Element {
    div()
        .size(
            Sizing::Fixed(1.5),
            Sizing::Fixed(role.size() * role.leading()),
        )
        .background(theme.accent)
}

/// Text an input method is still composing: tinted and underlined, which is
/// what every platform draws and therefore what everyone already reads as
/// "this is not typed yet".
fn composing(what: &str, theme: &Theme, role: Role) -> Element {
    row()
        .padding(Edges {
            top: 0.0,
            right: 0.0,
            bottom: 1.0,
            left: 0.0,
        })
        .border(0.0, theme.accent)
        .child(text(what.to_string(), role, theme.accent))
}

/// The innermost scrolling box under `point`, and how far it can scroll.
fn collect_scrollables(
    tree: &TaffyTree<Label>,
    node: NodeId,
    element: &Element,
    offset: (f32, f32),
    found: &mut Option<(Id, f32)>,
    point: (f32, f32),
) {
    let Ok(layout) = tree.layout(node) else {
        return;
    };
    let at = (offset.0 + layout.location.x, offset.1 + layout.location.y);
    let bounds = Rect::from_edges(
        at.0,
        at.1,
        at.0 + layout.size.width,
        at.1 + layout.size.height,
    );
    if element.style.scroll
        && let Some(id) = element.id
        && bounds.contains(Point::new(point.0, point.1))
    {
        // How much taller the contents are than the box holding them, measured
        // from the children rather than asked of taffy: they are laid out
        // normally and simply reach past the end.
        let content = tree
            .children(node)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|child| tree.layout(child).ok())
            .map(|child| child.location.y + child.size.height)
            .fold(0.0f32, f32::max);
        *found = Some((id, content - layout.size.height));
    }
    let children = tree.children(node).unwrap_or_default();
    for (child_node, child) in children.into_iter().zip(element.children.iter()) {
        collect_scrollables(tree, child_node, child, at, found, point);
    }
}
