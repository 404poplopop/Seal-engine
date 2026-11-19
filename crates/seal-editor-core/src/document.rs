/// One text replacement. `deleted` is what was in the document at `start`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextOp {
    pub start: usize,
    pub deleted: String,
    pub inserted: String,
}

impl TextOp {
    pub fn apply(&self, text: &mut String) {
        let end = self.start + self.deleted.len();
        text.replace_range(self.start..end, &self.inserted);
    }

    pub fn invert(&self) -> Self {
        Self { start: self.start, deleted: self.inserted.clone(), inserted: self.deleted.clone() }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDocument {
    pub id: String,
    pub saved: String,
    pub text: String,
    undo: Vec<TextOp>,
    redo: Vec<TextOp>,
}

impl TextDocument {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            saved: String::new(),
            text: String::new(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// The text on screen is now the saved copy.
    pub fn mark_saved(&mut self) {
        self.saved.clone_from(&self.text);
        self.clear_history();
    }

    pub fn edit(&mut self, start: usize, end: usize, insert: &str) -> TextOp {
        let deleted = self.text.get(start..end).unwrap_or("").to_string();
        let op = TextOp { start, deleted, inserted: insert.to_string() };
        op.apply(&mut self.text);
        self.push(op.clone());
        op
    }

    pub fn replace_with(&mut self, new_text: &str) -> Option<TextOp> {
        let op = diff_op(&self.text, new_text)?;
        op.apply(&mut self.text);
        self.push(op.clone());
        Some(op)
    }

    pub fn replay(&mut self, kind: &JournalStep) {
        match kind {
            JournalStep::Edit(op) => {
                op.apply(&mut self.text);
                self.push(op.clone());
            }
            JournalStep::Undo => {
                self.undo();
            }
            JournalStep::Redo => {
                self.redo();
            }
        }
    }

    pub fn undo(&mut self) -> bool {
        let Some(op) = self.undo.pop() else {
            return false;
        };
        op.invert().apply(&mut self.text);
        self.redo.push(op);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(op) = self.redo.pop() else {
            return false;
        };
        op.apply(&mut self.text);
        self.undo.push(op);
        true
    }

    fn push(&mut self, op: TextOp) {
        if let Some(last) = self.undo.last_mut() {
            if last.deleted.is_empty()
                && op.deleted.is_empty()
                && last.start + last.inserted.len() == op.start
            {
                last.inserted.push_str(&op.inserted);
                self.redo.clear();
                return;
            }
        }
        self.undo.push(op);
        self.redo.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JournalStep {
    Edit(TextOp),
    Undo,
    Redo,
}

fn diff_op(old: &str, new: &str) -> Option<TextOp> {
    if old == new {
        return None;
    }
    let old_b = old.as_bytes();
    let new_b = new.as_bytes();
    let mut start = 0;
    while start < old_b.len() && start < new_b.len() && old_b[start] == new_b[start] {
        start += 1;
    }
    while start > 0 && !old.is_char_boundary(start) {
        start -= 1;
    }
    let mut old_end = old_b.len();
    let mut new_end = new_b.len();
    while old_end > start && new_end > start && old_b[old_end - 1] == new_b[new_end - 1] {
        old_end -= 1;
        new_end -= 1;
    }
    while old_end < old.len() && !old.is_char_boundary(old_end) {
        old_end += 1;
    }
    while new_end < new.len() && !new.is_char_boundary(new_end) {
        new_end += 1;
    }
    if old_end < start {
        old_end = start;
    }
    if new_end < start {
        new_end = start;
    }
    Some(TextOp {
        start,
        deleted: old[start..old_end].to_string(),
        inserted: new[start..new_end].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_and_merged_typing() {
        let mut doc = TextDocument::new("scratch");
        doc.edit(0, 0, "a");
        doc.edit(1, 1, "b");
        assert_eq!(doc.text, "ab");
        assert_eq!(doc.undo_len(), 1, "adjacent inserts merge");
        assert!(doc.undo());
        assert_eq!(doc.text, "");
        assert!(doc.redo());
        assert_eq!(doc.text, "ab");
        doc.edit(2, 2, "!");
        doc.edit(0, 1, "");
        assert_eq!(doc.text, "b!");
        assert!(doc.undo());
        assert_eq!(doc.text, "ab!");
    }

    #[test]
    fn replay_restores_undo_history() {
        let mut live = TextDocument::new("scratch");
        let first = live.edit(0, 0, "hi");
        let second = live.edit(2, 2, "!");
        assert!(live.undo());
        let mut restored = TextDocument::new("scratch");
        restored.replay(&JournalStep::Edit(first));
        restored.replay(&JournalStep::Edit(second));
        restored.replay(&JournalStep::Undo);
        assert_eq!(restored.text, live.text);
        assert!(restored.redo());
        assert_eq!(restored.text, "hi!");
    }
}
