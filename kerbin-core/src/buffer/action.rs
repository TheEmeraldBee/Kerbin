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
        if self.row > buf.lines.len() {
            ActionResult::none(false)
        } else {
            let start = buf.get_edit_part(self.row, self.col);

            buf.lines[self.row].insert_str(self.col, &self.content);

            let end = buf.get_edit_part(self.row, self.col + self.content.len());

            buf.register_input_edit(start, start, end);

            let inverse = Box::new(Delete {
                row: self.row,
                col: self.col,

                len: self.content.chars().count(),
            });

            ActionResult::new(true, inverse)
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
        if self.row > buf.lines.len() {
            ActionResult::none(false)
        } else {
            let end = self
                .col
                .saturating_add(self.len)
                .min(buf.lines[self.row].chars().count());

            let start_edit = buf.get_edit_part(self.row, self.col);
            let end_edit = buf.get_edit_part(self.row, end);

            // Remove the chars from the string
            let removed: String = buf.lines[self.row].drain(self.col..end).collect::<String>();

            buf.register_input_edit(start_edit, end_edit, start_edit);

            let inverse = Box::new(Insert {
                row: self.row,
                col: self.col,
                content: removed,
            });

            ActionResult::new(true, inverse)
        }
    }
}

pub struct NoOp;
impl BufferAction for NoOp {
    extern "C" fn apply(&self, _buf: &mut TextBuffer) -> ActionResult {
        ActionResult::none(true)
    }
}
