//! One editable string, and everything that can be done to it.
//!
//! No rendering and no events: an [`Editor`] is a string, where the caret is,
//! where the selection started, and what the input method is in the middle of
//! composing. That makes the interesting parts -- what backspace does to a
//! selection, where the caret lands after a word jump, what happens when a
//! Hangul syllable is half typed -- testable without a window.
//!
//! Indices are byte offsets into the string, which is what everything that
//! touches Rust text uses, and every operation moves them by whole **grapheme
//! clusters** rather than by chars. A char is not a character: `가` typed on a
//! decomposing keyboard is three of them, an emoji with a skin tone is two, and
//! a backspace that eats one leaves a fragment on screen.

use unicode_segmentation::UnicodeSegmentation;

/// What the input method is composing, before it is committed.
///
/// Hangul, Japanese and Chinese are all typed this way: keystrokes go to the
/// system's input method, which hands back a string that is not yet part of
/// the document and replaces it as it changes. It is drawn at the caret and
/// underlined, and it is *not* in [`Editor::text`] -- putting it there would
/// mean every reader of the field seeing half-finished syllables.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preedit {
    pub text: String,
    /// The input method's own caret inside the composing text, in bytes.
    pub cursor: Option<(usize, usize)>,
}

/// An editable string.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Editor {
    text: String,
    caret: usize,
    /// Where a selection began. Equal to the caret when there is no selection.
    anchor: usize,
    preedit: Preedit,
}

impl Editor {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let end = text.len();
        Self {
            text,
            caret: end,
            anchor: end,
            preedit: Preedit::default(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn preedit(&self) -> &Preedit {
        &self.preedit
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.preedit.text.is_empty()
    }

    /// The selected range, low to high, or `None` when nothing is selected.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.caret != self.anchor)
            .then(|| (self.caret.min(self.anchor), self.caret.max(self.anchor)))
    }

    pub fn selected(&self) -> &str {
        match self.selection() {
            Some((from, to)) => &self.text[from..to],
            None => "",
        }
    }

    /// Replaces everything, putting the caret at the end.
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.caret = self.text.len();
        self.anchor = self.caret;
        self.preedit = Preedit::default();
    }

    /// Takes the text and leaves the field empty, which is what sending does.
    pub fn take(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        self.caret = 0;
        self.anchor = 0;
        self.preedit = Preedit::default();
        text
    }

    /// What the input method is composing now. An empty string clears it.
    pub fn compose(&mut self, text: String, cursor: Option<(usize, usize)>) {
        self.preedit = Preedit { text, cursor };
    }

    /// Inserts committed text, replacing the selection and ending composition.
    pub fn insert(&mut self, what: &str) {
        self.delete_selection();
        self.text.insert_str(self.caret, what);
        self.caret += what.len();
        self.anchor = self.caret;
        self.preedit = Preedit::default();
    }

    /// Backspace.
    ///
    /// A selection is deleted whole. Otherwise one grapheme goes, which is why
    /// this is not `pop`: backspacing a syllable should take the syllable.
    pub fn backspace(&mut self) {
        if self.delete_selection() {
            return;
        }
        if let Some(previous) = self.grapheme_before(self.caret) {
            self.text.replace_range(previous..self.caret, "");
            self.caret = previous;
            self.anchor = previous;
        }
    }

    /// Forward delete.
    pub fn delete(&mut self) {
        if self.delete_selection() {
            return;
        }
        if let Some(next) = self.grapheme_after(self.caret) {
            self.text.replace_range(self.caret..next, "");
        }
    }

    /// Moves the caret one grapheme. `select` extends the selection instead of
    /// collapsing it.
    pub fn move_left(&mut self, select: bool) {
        // Without a modifier, an arrow key against a selection collapses it to
        // that end rather than moving from the caret. Everything else that
        // edits text does this and it is startling when something does not.
        let to = match (select, self.selection()) {
            (false, Some((from, _))) => from,
            _ => self.grapheme_before(self.caret).unwrap_or(0),
        };
        self.place(to, select);
    }

    pub fn move_right(&mut self, select: bool) {
        let to = match (select, self.selection()) {
            (false, Some((_, to))) => to,
            _ => self.grapheme_after(self.caret).unwrap_or(self.text.len()),
        };
        self.place(to, select);
    }

    /// To the start of the word before the caret, the way alt+left does.
    pub fn move_word_left(&mut self, select: bool) {
        // The last word that *begins* before the caret. Testing where a word
        // ends instead skips the word the caret is sitting in the middle or at
        // the end of, which is the one alt+left is for.
        let mut at = 0;
        for (start, word) in self.text.split_word_bound_indices() {
            if start >= self.caret {
                break;
            }
            if !word.trim().is_empty() {
                at = start;
            }
        }
        self.place(at, select);
    }

    pub fn move_word_right(&mut self, select: bool) {
        let mut at = self.text.len();
        for (start, word) in self.text.split_word_bound_indices() {
            if start > self.caret && !word.trim().is_empty() {
                at = start + word.len();
                break;
            }
        }
        self.place(at, select);
    }

    pub fn move_home(&mut self, select: bool) {
        self.place(0, select);
    }

    pub fn move_end(&mut self, select: bool) {
        self.place(self.text.len(), select);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.text.len();
    }

    /// Puts the caret at a byte offset, snapped to a grapheme boundary.
    pub fn place(&mut self, at: usize, select: bool) {
        let at = self.snap(at);
        self.caret = at;
        if !select {
            self.anchor = at;
        }
    }

    fn place_both(&mut self, at: usize) {
        self.place(at, false);
    }

    /// Removes the selection if there is one, and says whether it did.
    fn delete_selection(&mut self) -> bool {
        match self.selection() {
            Some((from, to)) => {
                self.text.replace_range(from..to, "");
                self.place_both(from);
                true
            }
            None => false,
        }
    }

    /// The nearest grapheme boundary at or before `at`.
    fn snap(&self, at: usize) -> usize {
        let at = at.min(self.text.len());
        if self.text.is_char_boundary(at)
            && self
                .text
                .grapheme_indices(true)
                .any(|(start, _)| start == at)
        {
            return at;
        }
        if at == self.text.len() {
            return at;
        }
        self.text
            .grapheme_indices(true)
            .map(|(start, _)| start)
            .rfind(|start| *start <= at)
            .unwrap_or(0)
    }

    fn grapheme_before(&self, at: usize) -> Option<usize> {
        self.text
            .grapheme_indices(true)
            .map(|(start, _)| start)
            .rfind(|start| *start < at)
    }

    fn grapheme_after(&self, at: usize) -> Option<usize> {
        self.text
            .grapheme_indices(true)
            .map(|(start, grapheme)| start + grapheme.len())
            .find(|end| *end > at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_puts_text_where_the_caret_is() {
        let mut editor = Editor::new("ac");
        editor.place(1, false);
        editor.insert("b");
        assert_eq!(editor.text(), "abc");
        assert_eq!(editor.caret(), 2);
    }

    #[test]
    fn backspace_takes_a_whole_syllable() {
        // The reason this counts graphemes rather than chars. Precomposed
        // Hangul is one char per syllable, but decomposed it is three, and a
        // backspace that ate one would leave a jamo fragment on screen.
        let mut editor = Editor::new("한글");
        editor.backspace();
        assert_eq!(editor.text(), "한");
        editor.backspace();
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn backspace_takes_a_whole_emoji() {
        let mut editor = Editor::new("hi 👩‍💻");
        editor.backspace();
        assert_eq!(editor.text(), "hi ", "half an emoji was left behind");
    }

    #[test]
    fn backspace_at_the_start_does_nothing_rather_than_panicking() {
        let mut editor = Editor::new("");
        editor.backspace();
        assert_eq!(editor.text(), "");
        assert_eq!(editor.caret(), 0);
    }

    #[test]
    fn a_selection_is_replaced_by_what_is_typed() {
        let mut editor = Editor::new("hello world");
        editor.place(0, false);
        editor.place(5, true);
        assert_eq!(editor.selected(), "hello");
        editor.insert("goodbye");
        assert_eq!(editor.text(), "goodbye world");
        assert_eq!(editor.selection(), None);
    }

    #[test]
    fn backspace_with_a_selection_takes_the_selection_and_no_more() {
        let mut editor = Editor::new("hello world");
        editor.place(5, false);
        editor.place(11, true);
        editor.backspace();
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn an_arrow_key_collapses_a_selection_rather_than_moving_from_the_caret() {
        // What every other text field does. Moving from the caret instead
        // leaves the selection's far end where it was, which reads as the
        // cursor jumping.
        let mut editor = Editor::new("hello");
        editor.place(1, false);
        editor.place(4, true);
        editor.move_left(false);
        assert_eq!(editor.caret(), 1);
        assert_eq!(editor.selection(), None);

        editor.place(1, false);
        editor.place(4, true);
        editor.move_right(false);
        assert_eq!(editor.caret(), 4);
    }

    #[test]
    fn shift_and_an_arrow_extends_the_selection() {
        let mut editor = Editor::new("hello");
        editor.move_home(false);
        editor.move_right(true);
        editor.move_right(true);
        assert_eq!(editor.selected(), "he");
    }

    #[test]
    fn the_caret_cannot_be_moved_off_either_end() {
        let mut editor = Editor::new("ab");
        editor.move_home(false);
        editor.move_left(false);
        assert_eq!(editor.caret(), 0);
        editor.move_end(false);
        editor.move_right(false);
        assert_eq!(editor.caret(), 2);
    }

    #[test]
    fn a_word_jump_lands_between_words() {
        let mut editor = Editor::new("the quick brown");
        editor.move_end(false);
        editor.move_word_left(false);
        assert_eq!(&editor.text()[editor.caret()..], "brown");
        editor.move_word_left(false);
        assert_eq!(&editor.text()[editor.caret()..], "quick brown");
    }

    #[test]
    fn composing_text_stays_out_of_the_document() {
        // The half-typed syllable belongs to the input method until it is
        // committed. Putting it in the string would mean anything reading the
        // field -- a send button, a length check -- seeing a fragment.
        let mut editor = Editor::new("");
        editor.compose("ㅎ".into(), None);
        assert_eq!(editor.text(), "");
        assert_eq!(editor.preedit().text, "ㅎ");
        assert!(!editor.is_empty(), "there is something being typed");

        editor.compose("한".into(), None);
        assert_eq!(editor.text(), "");
        editor.insert("한");
        assert_eq!(editor.text(), "한");
        assert_eq!(editor.preedit().text, "", "committing ends the composition");
    }

    #[test]
    fn placing_the_caret_inside_a_syllable_snaps_it_to_the_edge() {
        // A click lands on a pixel, and a pixel can be halfway through a
        // three-byte character. Slicing there panics.
        let mut editor = Editor::new("한글");
        editor.place(1, false);
        assert_eq!(editor.caret(), 0);
        editor.place(4, false);
        assert_eq!(editor.caret(), 3);
        editor.place(999, false);
        assert_eq!(editor.caret(), editor.text().len());
    }

    #[test]
    fn taking_the_text_leaves_the_field_ready_for_the_next_one() {
        let mut editor = Editor::new("send me");
        editor.compose("ㅎ".into(), None);
        assert_eq!(editor.take(), "send me");
        assert!(editor.is_empty());
        assert_eq!(editor.caret(), 0);
        assert_eq!(editor.preedit().text, "");
    }

    #[test]
    fn select_all_covers_everything() {
        let mut editor = Editor::new("한글 and english");
        editor.select_all();
        assert_eq!(editor.selected(), editor.text());
    }
}
