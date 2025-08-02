use std::rc::Rc;

use rune::{Any, Value, alloc::clone::TryClone, runtime::Function};
use serde::{Deserialize, Serialize};
use stategine::prelude::Command;

use crate::{ConfigManager, GrammarManager, Running, Theme, buffer::Buffers, mode::Mode};

#[derive(Default)]
pub struct CommandStatus {
    pub success: bool,
}

#[derive(Debug)]
pub enum SpecialCommand {
    RunFunction(Rc<Function>, Value),
}

impl Command for SpecialCommand {
    fn apply(self: Box<Self>, engine: &mut stategine::Engine) {
        match *self {
            SpecialCommand::RunFunction(f, a) => match ConfigManager::run_function(engine, f, a) {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!("{e}");
                }
            },
        }
    }
}

#[derive(Debug, Any, TryClone, Serialize, Deserialize)]
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
    #[rune(constructor)]
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
                let buffers = engine.get_state_mut::<Buffers>();
                let success = buffers.cur_buffer().borrow_mut().move_cursor(x, y);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::ChangeMode(m) => engine.get_state_mut::<Mode>().0 = m,
            EditorCommand::InsertChar(chr) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .insert_char_at_cursor(chr);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::DeleteChars(offset, count) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .remove_chars_relative(offset, count);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::InsertLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .insert_newline_relative(offset);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::CreateLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .create_line(offset);
                engine.get_state_mut::<CommandStatus>().success = success;
            }
            EditorCommand::DeleteLine(offset) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
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
                    .cur_buffer()
                    .borrow_mut()
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

            EditorCommand::RefreshHighlights => {
                engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .refresh_highlights(&engine.get_state::<Theme>());
            }

            EditorCommand::Scroll(dist) => {
                let success = engine
                    .get_state_mut::<Buffers>()
                    .cur_buffer()
                    .borrow_mut()
                    .scroll_lines(dist);
                engine.get_state_mut::<CommandStatus>().success = success;
            }

            EditorCommand::StartChangeGroup => engine
                .get_state_mut::<Buffers>()
                .cur_buffer()
                .borrow_mut()
                .start_change_group(),
            EditorCommand::CommitChangeGroup => engine
                .get_state_mut::<Buffers>()
                .cur_buffer()
                .borrow_mut()
                .commit_change_group(),
            EditorCommand::Undo => engine
                .get_state_mut::<Buffers>()
                .cur_buffer()
                .borrow_mut()
                .undo(),
            EditorCommand::Redo => engine
                .get_state_mut::<Buffers>()
                .cur_buffer()
                .borrow_mut()
                .redo(),
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
