use stategine::prelude::Command;

use crate::{
    Running,
    buffer::{Buffers, TextBuffer},
    mode::Mode,
};

#[derive(Clone, Debug)]
pub enum EditorCommand {
    MoveCursor(i16, i16),
    ChangeMode(char),
    InsertChar(char),
    DeleteChars(i16, usize),
    InsertLine(i16),
    CreateLine(i16),
    DeleteLine(i16),
    CloseCurrentBuffer,
    ChangeBuffer(isize),
    WriteFile(Option<String>),
    OpenFile(String),
    Quit,

    // History commands
    StartChangeGroup,
    CommitChangeGroup,
    Undo,
    Redo,
}

impl Command for EditorCommand {
    fn apply(self: Box<Self>, engine: &mut stategine::Engine) {
        match *self {
            EditorCommand::MoveCursor(x, y) => {
                let mut buffers = engine.get_state_mut::<Buffers>();
                buffers.cur_buffer_mut().move_cursor(x, y);
            }
            EditorCommand::ChangeMode(m) => engine.get_state_mut::<Mode>().0 = m,
            EditorCommand::InsertChar(chr) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .insert_char_at_cursor(chr);
            }
            EditorCommand::DeleteChars(offset, count) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .remove_chars_relative(offset, count);
            }
            EditorCommand::InsertLine(offset) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .insert_newline_relative(offset);
            }
            EditorCommand::CreateLine(offset) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .create_line(offset);
            }
            EditorCommand::DeleteLine(offset) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .delete_line(offset);
            }
            EditorCommand::CloseCurrentBuffer => {
                engine.get_state_mut::<Buffers>().close_current_buffer();
            }
            EditorCommand::ChangeBuffer(dist) => {
                engine.get_state_mut::<Buffers>().change_buffer(dist);
            }
            EditorCommand::WriteFile(path) => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .write_file(path);
            }
            EditorCommand::OpenFile(path) => engine.get_state_mut::<Buffers>().open(path),
            EditorCommand::Quit => {
                engine.get_state_mut::<Running>().0 = false;
            }

            // History command implementations
            EditorCommand::StartChangeGroup => engine
                .get_state_mut::<Buffers>()
                .cur_buffer_mut()
                .start_change_group(),
            EditorCommand::CommitChangeGroup => engine
                .get_state_mut::<Buffers>()
                .cur_buffer_mut()
                .commit_change_group(),
            EditorCommand::Undo => engine.get_state_mut::<Buffers>().cur_buffer_mut().undo(),
            EditorCommand::Redo => engine.get_state_mut::<Buffers>().cur_buffer_mut().redo(),
        }
    }
}
