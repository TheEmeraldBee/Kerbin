use std::str::FromStr;

use crokey::KeyCombination;
use rune::{Any, alloc::clone::TryClone};
use stategine::prelude::Command;

use crate::{
    EditorStyle, GrammarManager, Running, Theme, buffer::Buffers, input::InputConfig, mode::Mode,
};

#[derive(Default)]
pub struct CommandStatus {
    pub success: bool,
}

#[derive(Debug, Any, TryClone)]
#[allow(unused)]
pub enum EditorCommand {
    #[rune(constructor)]
    MoveCursor(#[rune(get, set)] i16, #[rune(get, set)] i16),
    #[rune(constructor)]
    ChangeMode(#[rune(get, set)] char),
    #[rune(constructor)]
    InsertChar(#[rune(get, set)] char),
    #[rune(constructor)]
    DeleteChars(#[rune(get, set)] i16, #[rune(get, set)] usize),
    #[rune(constructor)]
    InsertLine(#[rune(get, set)] i16),
    #[rune(constructor)]
    CreateLine(#[rune(get, set)] i16),
    #[rune(constructor)]
    DeleteLine(#[rune(get, set)] i16),
    #[rune(constructor)]
    CloseCurrentBuffer,
    #[rune(constructor)]
    CloseBuffer(#[rune(get, set)] usize),
    #[rune(constructor)]
    ChangeBuffer(#[rune(get, set)] isize),
    #[rune(constructor)]
    WriteFile(#[rune(get, set)] Option<String>),
    #[rune(constructor)]
    OpenFile(#[rune(get, set)] String),
    #[rune(constructor)]
    Quit,

    #[rune(constructor)]
    RegisterKeybinding(
        #[rune(get, set)] rune::alloc::Vec<char>,
        #[rune(get, set)] rune::alloc::Vec<String>,
        #[rune(get, set)] rune::alloc::Vec<EditorCommand>,
        #[rune(get, set)] String,
    ),

    #[rune(constructor)]
    RegisterLanguageExt(#[rune(get, set)] String, #[rune(get, set)] String),

    #[rune(constructor)]
    RegisterTheme(#[rune(get, set)] String, #[rune(get, set)] EditorStyle),

    #[rune(constructor)]
    Scroll(#[rune(get, set)] isize),

    #[rune(constructor)]
    RefreshHighlights,

    // History commands
    #[rune(constructor)]
    StartChangeGroup,
    #[rune(constructor)]
    CommitChangeGroup,
    #[rune(constructor)]
    Undo,
    #[rune(constructor)]
    Redo,

    // Repetition command
    Repeat(
        #[rune(get, set)] rune::alloc::Vec<EditorCommand>,
        #[rune(get, set)] usize,
    ),
}

impl Clone for EditorCommand {
    fn clone(&self) -> Self {
        self.try_clone().unwrap()
    }
}

impl Command for EditorCommand {
    fn apply(self: Box<Self>, engine: &mut stategine::Engine) {
        engine.get_state_mut::<CommandStatus>().success = true;

        match *self {
            EditorCommand::MoveCursor(x, y) => {
                let mut buffers = engine.get_state_mut::<Buffers>();
                let success = buffers.cur_buffer_mut().move_cursor(x, y);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::ChangeMode(m) => engine.get_state_mut::<Mode>().0 = m,
            EditorCommand::InsertChar(chr) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .insert_char_at_cursor(chr);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::DeleteChars(offset, count) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .remove_chars_relative(offset, count);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::InsertLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .insert_newline_relative(offset);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::CreateLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .create_line(offset);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::DeleteLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .delete_line(offset);
                engine.get_state_mut::<CommandStatus>().success = success;
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
                let theme = engine.get_state::<Theme>();

                engine
                    .get_state_mut::<Buffers>()
                    .open(path, &mut grammar, &theme)
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
                    modes.to_vec(),
                    key_sequence.to_vec(),
                    commands.to_vec(),
                    desc,
                )
            }

            EditorCommand::RefreshHighlights => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .refresh_highlights(&engine.get_state::<Theme>());
            }

            EditorCommand::RegisterLanguageExt(ext, lang) => {
                engine
                    .get_state_mut::<GrammarManager>()
                    .register_extension(ext, lang);
            }

            EditorCommand::RegisterTheme(key, style) => {
                engine.get_state_mut::<Theme>().register(key, style);
            }

            EditorCommand::Scroll(dist) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer_mut()
                    .scroll_lines(dist);
                engine.get_state_mut::<CommandStatus>().success = success;
            }

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
            EditorCommand::Repeat(commands, count) => {
                for _ in 0..count {
                    for command in &commands {
                        Box::new(command.clone()).apply(engine);
                        if !engine.get_state::<CommandStatus>().success {
                            engine.get_state_mut::<CommandStatus>().success = false;
                            return;
                        }
                    }
                }
                engine.get_state_mut::<CommandStatus>().success = true;
            }
        }
    }
}
