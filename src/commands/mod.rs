use std::sync::{Arc, atomic::Ordering};

use rune::{Any, alloc::clone::TryClone};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Default)]
pub struct CommandStatus {
    pub success: bool,
}

//#[derive(Debug)]
//pub enum SpecialCommand {
//    RunFunction(Rc<Function>, Value),
//}

//impl Command for SpecialCommand {
//    fn apply(self: Box<Self>, state: Arc<AppState>) {
//        match *self {
//            SpecialCommand::RunFunction(f, a) => match ConfigManager::run_function(engine, f, a) {
//                Ok(_) => {}
//                Err(e) => {
//                    tracing::error!("{e}");
//                }
//            },
//        }
//    }
//}

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

impl EditorCommand {
    pub fn apply(self, state: Arc<AppState>) {
        //state.get_state_mut::<CommandStatus>().success = true;

        match self {
            EditorCommand::MoveCursor(x, y) => {
                let buffers = state.buffers.read().unwrap();
                let success = buffers.cur_buffer().borrow_mut().move_cursor(x, y);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::ChangeMode(m) => state.mode.store(u32::from(m), Ordering::Relaxed),
            EditorCommand::InsertChar(chr) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .insert_char_at_cursor(chr);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::DeleteChars(offset, count) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .remove_chars_relative(offset, count);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::InsertLine(offset) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .insert_newline_relative(offset);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::CreateLine(offset) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .create_line(offset);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::DeleteLine(offset) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .delete_line(offset);
                state.command_success.store(success, Ordering::Relaxed);
            }
            EditorCommand::CloseCurrentBuffer => {
                state.buffers.write().unwrap().close_current_buffer();
            }
            EditorCommand::CloseBuffer(idx) => {
                state.buffers.write().unwrap().close_buffer(idx);
            }
            EditorCommand::ChangeBuffer(dist) => {
                state.buffers.write().unwrap().change_buffer(dist);
            }
            EditorCommand::WriteFile(path) => {
                state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .write_file(path);
            }
            EditorCommand::OpenFile(path) => {
                let mut grammar = state.grammar.write().unwrap();
                let theme = state.theme.read().unwrap();

                state
                    .buffers
                    .write()
                    .unwrap()
                    .open(path, &mut grammar, &theme)
            }
            EditorCommand::Quit => {
                state.running.store(false, Ordering::Relaxed);
            }

            EditorCommand::RefreshHighlights => {
                state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .refresh_highlights(&state.theme.read().unwrap());
            }

            EditorCommand::Scroll(dist) => {
                let success = state
                    .buffers
                    .read()
                    .unwrap()
                    .cur_buffer()
                    .borrow_mut()
                    .scroll_lines(dist);
                state.command_success.store(success, Ordering::Relaxed);
            }

            EditorCommand::StartChangeGroup => state
                .buffers
                .read()
                .unwrap()
                .cur_buffer()
                .borrow_mut()
                .start_change_group(),
            EditorCommand::CommitChangeGroup => state
                .buffers
                .read()
                .unwrap()
                .cur_buffer()
                .borrow_mut()
                .commit_change_group(),
            EditorCommand::Undo => state
                .buffers
                .read()
                .unwrap()
                .cur_buffer()
                .borrow_mut()
                .undo(),
            EditorCommand::Redo => state
                .buffers
                .read()
                .unwrap()
                .cur_buffer()
                .borrow_mut()
                .redo(),
            EditorCommand::Repeat(commands, count) => {
                for _ in 0..count {
                    for command in &commands {
                        command.clone().apply(state.clone());
                        if !state.command_success.load(Ordering::Relaxed) {
                            state.command_success.store(false, Ordering::Relaxed);
                            return;
                        }
                    }
                }
                state.command_success.store(true, Ordering::Relaxed);
            }
        }
    }
}
