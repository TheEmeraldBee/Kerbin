use super::TextBuffer;

pub struct ActionResult {
    pub success: bool,
    pub action: Box<dyn BufferAction>,
}

impl ActionResult {
    pub fn new(success: bool, action: Box<dyn BufferAction>) -> Self {
        Self { success, action }
    }

    pub fn none(success: bool) -> Self {
        Self::new(success, Box::new(NoOp))
    }
}

pub trait BufferAction: Send + Sync {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult;
}

pub struct Insert {
    pub row: usize,
    pub col: usize,

    pub content: String,
}

impl BufferAction for Insert {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if let Some(line) = buf.lines.get_mut(self.row) {
            // Borrow string as core rust string
            line.insert_str(self.col, &self.content);

            let inverse = Box::new(Delete {
                row: self.row,
                col: self.col,

                len: self.content.chars().count(),
            });

            ActionResult::new(true, inverse)
        } else {
            ActionResult::none(false)
        }
    }
}

pub struct Delete {
    pub row: usize,
    pub col: usize,

    pub len: usize,
}

impl BufferAction for Delete {
    extern "C" fn apply(&self, buf: &mut TextBuffer) -> ActionResult {
        if let Some(line) = buf.lines.get_mut(self.row) {
            let end = self.col.saturating_add(self.len).min(line.chars().count());

            // Remove the chars from the string
            let removed: String = line
                .drain(self.col..end)
                .collect::<std::string::String>()
                .into();

            let inverse = Box::new(Insert {
                row: self.row,
                col: self.col,
                content: removed,
            });

            ActionResult::new(true, inverse)
        } else {
            ActionResult::none(false)
        }
    }
}

pub struct NoOp;
impl BufferAction for NoOp {
    extern "C" fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
