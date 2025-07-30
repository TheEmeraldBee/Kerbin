use std::str::FromStr;

use crokey::KeyCombination;
use rune::FromValue;
use stategine::prelude::Command;

use crate::{
    GrammarManager, HighlightConfiguration, Running, buffer::Buffers, input::InputConfig,
    mode::Mode,
};

#[derive(Clone, Debug, FromValue)]
#[allow(unused)]
pub enum EditorCommand {
    MoveCursor(i16, i16),
    ChangeMode(char),
    InsertChar(char),
    DeleteChars(i16, usize),
    InsertLine(i16),
    CreateLine(i16),
    DeleteLine(i16),
    CloseCurrentBuffer,
    CloseBuffer(usize),
    ChangeBuffer(isize),
    WriteFile(Option<String>),
    OpenFile(String),
    Quit,

    RegisterKeybinding(Vec<char>, Vec<String>, Vec<EditorCommand>, String),

    RegisterLanguageExt(String, String),

    Scroll(isize),

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
            EditorCommand::CloseBuffer(idx) => {
                engine.get_state_mut::<Buffers>().close_buffer(idx);
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
            EditorCommand::OpenFile(path) => {
                let mut grammar = engine.get_state_mut::<GrammarManager>();
                let hl_config = engine.get_state::<HighlightConfiguration>();

                engine
                    .get_state_mut::<Buffers>()
                    .open(path, &mut grammar, &hl_config)
            }
            EditorCommand::Quit => {
                engine.get_state_mut::<Running>().0 = false;
            }

            EditorCommand::RegisterKeybinding(modes, sequence, commands, desc) => {
                let mut key_sequence = vec![];
                for key in &sequence {
                    key_sequence.push(match KeyCombination::from_str(key) {
                        Ok(t) => t,
                        Err(e) => {
                            tracing::error!("Failed to add keybinding due to: {e}");
                            return;
                        }
                    })
                }
                engine.get_state_mut::<InputConfig>().register_input(
                    modes,
                    key_sequence,
                    commands,
                    desc,
                )
            }

            EditorCommand::RegisterLanguageExt(ext, lang) => {
                engine
                    .get_state_mut::<GrammarManager>()
                    .register_extension(ext, lang);
            }

            EditorCommand::Scroll(dist) => engine
                .get_state_mut::<Buffers>()
                .cur_buffer_mut()
                .scroll_lines(dist),

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
