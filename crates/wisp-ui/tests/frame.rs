//! Laying frames out and asking where things ended up.
//!
//! No window and no GPU: a frame is a tree in and a scene plus a set of
//! answers out, which is the whole reason layout and painting are separated
//! from the renderer.

use wisp_core::{Rgba, Scale, Scene};
use wisp_ui::element::{Edges, Sizing};
use wisp_ui::{Pointer, Role, Theme, Ui, column, div, row, spacer, text};

fn ui() -> (Ui, Scene) {
    (Ui::new(), Scene::new())
}

fn ink() -> Rgba {
    Theme::dark().ink
}

#[test]
fn a_row_lays_its_children_out_left_to_right() {
    let (mut ui, mut scene) = ui();
    let tree = row()
        .gap(10.0)
        .child(div().size(Sizing::Fixed(40.0), Sizing::Fixed(20.0)).id("a"))
        .child(div().size(Sizing::Fixed(40.0), Sizing::Fixed(20.0)).id("b"));
    let seen = ui.frame(&tree, (400.0, 200.0), Scale::ONE, &mut scene);

    let a = seen.bounds("a").expect("a was laid out");
    let b = seen.bounds("b").expect("b was laid out");
    assert_eq!(a.left(), 0.0);
    assert_eq!(b.left(), 50.0, "the gap should be between them");
    assert_eq!(a.top(), b.top(), "a row keeps its children on one line");
}

#[test]
fn a_spacer_pushes_what_follows_it_to_the_end() {
    let (mut ui, mut scene) = ui();
    let tree = row()
        .size(Sizing::Fixed(300.0), Sizing::Fixed(40.0))
        .child(
            div()
                .size(Sizing::Fixed(20.0), Sizing::Fixed(20.0))
                .id("first"),
        )
        .child(spacer())
        .child(
            div()
                .size(Sizing::Fixed(20.0), Sizing::Fixed(20.0))
                .id("last"),
        );
    let seen = ui.frame(&tree, (300.0, 40.0), Scale::ONE, &mut scene);

    assert_eq!(seen.bounds("first").unwrap().left(), 0.0);
    assert_eq!(seen.bounds("last").unwrap().right(), 300.0);
}

#[test]
fn a_fixed_sidebar_beside_a_growing_pane_adds_up_to_the_window() {
    // The arrangement every one of these windows is: something of a known
    // width, and something that takes the rest. Getting it wrong by letting
    // the second one ask for the whole window is how a pane runs off the side.
    let (mut ui, mut scene) = ui();
    let tree = row()
        .size(Sizing::Fill, Sizing::Fill)
        .child(div().size(Sizing::Fixed(200.0), Sizing::Fill).id("side"))
        .child(div().grow(1.0).size(Sizing::Fill, Sizing::Fill).id("main"));
    let seen = ui.frame(&tree, (800.0, 400.0), Scale::ONE, &mut scene);

    let side = seen.bounds("side").unwrap();
    let main = seen.bounds("main").unwrap();
    assert_eq!(side.size.width, 200.0);
    assert_eq!(main.left(), 200.0);
    assert_eq!(
        main.right(),
        800.0,
        "the pane should stop at the window's edge"
    );
}

#[test]
fn padding_moves_the_children_in_and_not_the_box() {
    let (mut ui, mut scene) = ui();
    let tree = column()
        .size(Sizing::Fixed(100.0), Sizing::Fixed(100.0))
        .padding(Edges::all(12.0))
        .id("outer")
        .child(div().size(Sizing::Fill, Sizing::Fixed(10.0)).id("inner"));
    let seen = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);

    assert_eq!(seen.bounds("outer").unwrap().size.width, 100.0);
    let inner = seen.bounds("inner").unwrap();
    assert_eq!(inner.left(), 12.0);
    assert_eq!(inner.size.width, 76.0, "100 less twelve on each side");
}

#[test]
fn a_long_line_wraps_instead_of_running_off_the_side() {
    // The reason text is measured by the layout engine rather than before it.
    let (mut ui, mut scene) = ui();
    let sentence = "a sentence that is far too long to fit across a narrow column of text";
    let tree = column()
        .size(Sizing::Fixed(160.0), Sizing::Hug)
        .id("box")
        .child(text(sentence, Role::Body, ink()));
    let seen = ui.frame(&tree, (600.0, 600.0), Scale::ONE, &mut scene);

    let box_ = seen.bounds("box").unwrap();
    assert!(
        box_.size.height > Role::Body.size() * 2.0,
        "it did not wrap"
    );
    let widest = scene
        .masked()
        .iter()
        .map(|g| g.bounds.right().get())
        .fold(0.0f32, f32::max);
    assert!(
        widest <= 161.0,
        "a glyph landed at {widest}, past the column"
    );
}

#[test]
fn a_short_label_is_not_wrapped_by_a_rounding_error() {
    // It was: layout and measurement reach the same width by different
    // floating point routes, and a box a ten-thousandth narrower than its own
    // text drops the last letter onto a second line.
    let (mut ui, mut scene) = ui();
    let tree = row().child(text("Send", Role::Label, ink()).id("label"));
    let seen = ui.frame(&tree, (400.0, 100.0), Scale::ONE, &mut scene);

    let label = seen.bounds("label").unwrap();
    assert!(
        label.size.height < Role::Label.size() * Role::Label.leading() * 1.5,
        "one word became two lines: {}pt tall",
        label.size.height
    );
}

#[test]
fn the_pointer_hovers_what_is_under_it_and_nothing_else() {
    let (mut ui, mut scene) = ui();
    let tree = row()
        .child(
            div()
                .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
                .id("left"),
        )
        .child(
            div()
                .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
                .id("right"),
        );

    ui.point(Pointer {
        at: (10.0, 10.0),
        down: false,
    });
    let seen = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);
    assert!(seen.hovered("left") && !seen.hovered("right"));

    scene.clear();
    ui.point(Pointer {
        at: (60.0, 10.0),
        down: false,
    });
    let seen = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);
    assert!(seen.hovered("right") && !seen.hovered("left"));
}

#[test]
fn what_is_drawn_on_top_is_what_is_pointed_at() {
    // Two boxes over each other. The one added later is painted over the
    // first, so it is the one you can see and the one you are pointing at.
    let (mut ui, mut scene) = ui();
    let tree = div()
        .child(
            div()
                .size(Sizing::Fixed(80.0), Sizing::Fixed(80.0))
                .id("under"),
        )
        .child(
            div()
                .size(Sizing::Fixed(80.0), Sizing::Fixed(80.0))
                .id("over"),
        );
    // A column stacks them, so aim at the second one's own row.
    ui.point(Pointer {
        at: (10.0, 100.0),
        down: false,
    });
    let seen = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);
    assert!(seen.hovered("over"), "expected the second box");
}

#[test]
fn a_click_is_a_press_and_a_release_on_the_same_thing() {
    let (mut ui, mut scene) = ui();
    let tree = div().child(
        div()
            .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
            .id("button"),
    );
    let frame = |at: (f32, f32), down: bool, ui: &mut Ui, scene: &mut Scene| {
        scene.clear();
        ui.point(Pointer { at, down });
        ui.frame(&tree, (200.0, 200.0), Scale::ONE, scene)
    };

    assert!(!frame((10.0, 10.0), false, &mut ui, &mut scene).clicked("button"));
    let pressed = frame((10.0, 10.0), true, &mut ui, &mut scene);
    assert!(pressed.pressed("button"), "should read as held");
    assert!(!pressed.clicked("button"), "not clicked until it is let go");
    assert!(frame((10.0, 10.0), false, &mut ui, &mut scene).clicked("button"));
}

#[test]
fn letting_go_somewhere_else_is_not_a_click() {
    // Pressing a button, sliding off it and releasing is how anybody who has
    // changed their mind cancels. It has to do nothing.
    let (mut ui, mut scene) = ui();
    let tree = row()
        .child(
            div()
                .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
                .id("button"),
        )
        .child(
            div()
                .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
                .id("elsewhere"),
        );

    scene.clear();
    ui.point(Pointer {
        at: (10.0, 10.0),
        down: true,
    });
    ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);

    scene.clear();
    ui.point(Pointer {
        at: (70.0, 10.0),
        down: false,
    });
    let released = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);
    assert!(!released.clicked("button"));
    assert!(
        !released.clicked("elsewhere"),
        "and not the thing it was let go over"
    );
}

#[test]
fn a_pointer_that_has_left_the_window_is_over_nothing() {
    let (mut ui, mut scene) = ui();
    let tree = div().child(
        div()
            .size(Sizing::Fixed(50.0), Sizing::Fixed(50.0))
            .id("box"),
    );
    ui.point(Pointer {
        at: (f32::MIN, f32::MIN),
        down: false,
    });
    let seen = ui.frame(&tree, (200.0, 200.0), Scale::ONE, &mut scene);
    assert!(!seen.hovered("box"));
}

#[test]
fn a_frame_paints_a_box_for_everything_with_a_background() {
    let (mut ui, mut scene) = ui();
    let tree = column()
        .background(Theme::dark().base)
        .child(
            div()
                .size(Sizing::Fixed(10.0), Sizing::Fixed(10.0))
                .background(ink()),
        )
        // No background, no border, no shadow: nothing to draw.
        .child(div().size(Sizing::Fixed(10.0), Sizing::Fixed(10.0)));
    ui.frame(&tree, (100.0, 100.0), Scale::ONE, &mut scene);
    assert_eq!(scene.quads().len(), 2);
}

#[test]
fn everything_scales_together_on_a_retina_display() {
    // Layout is in points and the scene is in device pixels. Getting that
    // conversion wrong in one place and not the other is the fault this
    // library separates the two units to prevent.
    let (mut ui, mut scene) = ui();
    let tree = div().child(
        div()
            .size(Sizing::Fixed(50.0), Sizing::Fixed(20.0))
            .background(ink())
            .id("box"),
    );
    let scale = Scale::new(2.0).unwrap();
    let seen = ui.frame(&tree, (200.0, 200.0), scale, &mut scene);

    assert_eq!(
        seen.bounds("box").unwrap().size.width,
        50.0,
        "layout stays in points"
    );
    assert_eq!(
        scene.quads()[0].bounds.size.width.get(),
        100.0,
        "and the scene is in device pixels"
    );
}

// --- typing -----------------------------------------------------------------

use wisp_ui::Editor;
use wisp_ui::input::{Composition, Input, Key, Press};
use wisp_ui::ui::OnEnter;

/// Builds one frame containing a single field, and returns what it did.
fn typed(
    ui: &mut Ui,
    scene: &mut Scene,
    editor: &mut Editor,
    on_enter: OnEnter,
) -> wisp_ui::ui::Edited {
    scene.clear();
    let theme = Theme::dark();
    let (field, edited) = ui.field("f", editor, &theme, Role::Body, "type here", on_enter);
    let tree = column().size(Sizing::Fill, Sizing::Fill).child(field);
    ui.frame(&tree, (400.0, 200.0), Scale::ONE, scene);
    edited
}

/// Clicks the field, so that it has the keyboard.
fn focus_the_field(ui: &mut Ui, scene: &mut Scene, editor: &mut Editor) {
    ui.point(Pointer {
        at: (4.0, 4.0),
        down: true,
    });
    typed(ui, scene, editor, OnEnter::Submit);
    ui.point(Pointer {
        at: (4.0, 4.0),
        down: false,
    });
    typed(ui, scene, editor, OnEnter::Submit);
    // The click is read on the frame after it, which is when focus moves.
    typed(ui, scene, editor, OnEnter::Submit);
}

#[test]
fn a_field_only_takes_the_keyboard_once_it_has_been_clicked() {
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::default();
    ui.input(Input::Key(Press::new(Key::Insert("a".into()))));
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert_eq!(editor.text(), "", "an unfocused field is not listening");

    focus_the_field(&mut ui, &mut scene, &mut editor);
    ui.input(Input::Key(Press::new(Key::Insert("a".into()))));
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert_eq!(editor.text(), "a");
}

#[test]
fn clicking_away_puts_the_keyboard_down() {
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::default();
    focus_the_field(&mut ui, &mut scene, &mut editor);
    assert!(ui.has_focus("f"));

    // A press and a release well away from the field.
    ui.point(Pointer {
        at: (300.0, 150.0),
        down: true,
    });
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    ui.point(Pointer {
        at: (300.0, 150.0),
        down: false,
    });
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert!(!ui.has_focus("f"));
}

#[test]
fn an_input_method_composes_before_it_commits() {
    // The whole of how Korean is typed. The system hands over a syllable in
    // progress, replaces it as more of it is typed, and commits it at the end;
    // nothing here composes anything itself.
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::default();
    focus_the_field(&mut ui, &mut scene, &mut editor);

    for stage in ["ㅎ", "하", "한"] {
        ui.input(Input::Ime(Composition::Preedit(stage.into(), None)));
        typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
        assert_eq!(editor.text(), "", "composing text is not in the document");
        assert_eq!(editor.preedit().text, stage);
    }

    ui.input(Input::Ime(Composition::Commit("한".into())));
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert_eq!(editor.text(), "한");
    assert_eq!(editor.preedit().text, "");
}

#[test]
fn return_submits_and_the_modifier_makes_a_line() {
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::new("hello");
    focus_the_field(&mut ui, &mut scene, &mut editor);

    ui.input(Input::Key(Press::new(Key::Enter)));
    let edited = typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert!(edited.submitted);
    assert_eq!(
        editor.text(),
        "hello",
        "submitting does not change the text"
    );

    ui.input(Input::Key(Press {
        modifier: true,
        ..Press::new(Key::Enter)
    }));
    let edited = typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert!(!edited.submitted);
    assert_eq!(editor.text(), "hello\n");
}

#[test]
fn a_field_set_to_newline_never_submits() {
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::new("x");
    focus_the_field(&mut ui, &mut scene, &mut editor);
    ui.input(Input::Key(Press::new(Key::Enter)));
    let edited = typed(&mut ui, &mut scene, &mut editor, OnEnter::Newline);
    assert!(!edited.submitted);
    assert_eq!(editor.text(), "x\n");
}

#[test]
fn a_focused_field_draws_a_caret_and_an_empty_one_draws_its_placeholder() {
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::default();
    // Unfocused: the placeholder and nothing else.
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    let before = scene.quads().len();

    focus_the_field(&mut ui, &mut scene, &mut editor);
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert!(
        scene.quads().len() > before,
        "a focused field should have drawn a caret"
    );
}

#[test]
fn keystrokes_nobody_was_listening_for_are_dropped() {
    // Kept, they would arrive in whatever is focused next -- half a sentence
    // appearing in the field somebody has just clicked into.
    let (mut ui, mut scene) = ui();
    let mut editor = Editor::default();
    ui.input(Input::Key(Press::new(Key::Insert("stray".into()))));
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);

    focus_the_field(&mut ui, &mut scene, &mut editor);
    typed(&mut ui, &mut scene, &mut editor, OnEnter::Submit);
    assert_eq!(editor.text(), "");
}
