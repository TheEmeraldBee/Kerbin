use std::time::{Duration, SystemTime};

use crate::*;
use ascii_forge::{prelude::*, window::Render};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    text.split_whitespace()
        .fold(vec![String::new()], |mut lines, word| {
            let current_line = lines.last_mut().unwrap();
            if current_line.len() + word.len() + 1 > max_width {
                lines.push(word.to_string());
            } else {
                if !current_line.is_empty() {
                    current_line.push(' ');
                }
                current_line.push_str(word);
            }
            lines
        })
}

pub async fn register_log_chunk(
    chunks: ResMut<Chunks>,
    window: Res<WindowState>,
    log: ResMut<LogState>,
) {
    get!(mut log);

    // Update internal state from receiver
    log.poll_messages();

    if log.entries().is_empty() {
        return;
    }

    get!(mut chunks, window);

    let layout = Layout::new()
        .row(flexible(), vec![flexible()])
        .row(percent(50.0), vec![flexible(), percent(100.0)])
        .calculate(window.size())
        .unwrap();
    chunks.register_chunk::<LogChunk>(2, layout[1][1]);
}

pub async fn render_log(log_chunk: Chunk<LogChunk>, log: ResMut<LogState>, theme: Res<Theme>) {
    let Some(mut chunk) = log_chunk.get().await else {
        return;
    };
    get!(mut log, theme);

    log.poll_messages();

    let size = chunk.size();
    let max_width = size.x as usize;
    let max_height = size.y;

    // Collect wrapped lines
    let mut all_lines = Vec::new();

    for msg in log.entries() {
        let wrapped_lines = wrap_text(&msg.message.message, max_width);

        for line in wrapped_lines {
            all_lines.push((msg, line));
        }
    }

    // Only render last N lines that fit
    let total_lines = all_lines.len().min(max_height as usize);
    let start_y = max_height - total_lines as u16;

    for (i, (msg, line)) in all_lines.iter().rev().take(total_lines).rev().enumerate() {
        let style = msg.message.style(&theme);

        // Align to right: calculate x offset
        let x = size.x.saturating_sub(line.len() as u16);
        let y = start_y + i as u16;

        render!(chunk, vec2(x, y) => [style.apply(line)]);
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Level {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub level: Level,
    pub origin: String,
    pub message: String,
}

impl Message {
    pub fn style(&self, theme: &Theme) -> ContentStyle {
        match self.level {
            Level::Low => theme.get_fallback_default(["ui.log.low", "ui.log", "ui.text"]),
            Level::Medium => theme.get_fallback_default(["ui.log.medium", "ui.log", "ui.text"]),
            Level::High => theme.get_fallback_default(["ui.log.high", "ui.log", "ui.text"]),
            Level::Critical => theme.get_fallback_default(["ui.log.critical", "ui.log", "ui.text"]),
        }
    }
}

pub fn low(origin: impl ToString, message: impl ToString) -> Message {
    let message = message.to_string();
    tracing::info!(message);

    Message {
        level: Level::Low,
        origin: origin.to_string(),
        message,
    }
}
pub fn medium(origin: impl ToString, message: impl ToString) -> Message {
    let message = message.to_string();
    tracing::warn!(message);

    Message {
        level: Level::Medium,
        origin: origin.to_string(),
        message,
    }
}
pub fn high(origin: impl ToString, message: impl ToString) -> Message {
    let message = message.to_string();
    tracing::error!(message);

    Message {
        level: Level::High,
        origin: origin.to_string(),
        message,
    }
}
pub fn critical(origin: impl ToString, message: impl ToString) -> Message {
    let message = message.to_string();
    tracing::error!(message);

    Message {
        level: Level::Critical,
        origin: origin.to_string(),
        message,
    }
}

#[derive(State)]
pub struct LogSender {
    pub(crate) sender: UnboundedSender<(Duration, Message)>,
}

impl LogSender {
    pub fn low(&self, origin: impl ToString, message: impl ToString) {
        let _ = self
            .sender
            .send((Duration::from_millis(3000), low(origin, message)));
    }

    pub fn medium(&self, origin: impl ToString, message: impl ToString) {
        let _ = self
            .sender
            .send((Duration::from_millis(5000), medium(origin, message)));
    }

    pub fn high(&self, origin: impl ToString, message: impl ToString) {
        let _ = self
            .sender
            .send((Duration::from_millis(8000), high(origin, message)));
    }

    pub fn critical(&self, origin: impl ToString, message: impl ToString) {
        let _ = self
            .sender
            .send((Duration::from_millis(10000), critical(origin, message)));
    }
}

pub struct TimedMessage {
    pub inserted: SystemTime,
    pub duration: Duration,
    pub message: Message,
}

#[derive(State, Default)]
pub struct LogState {
    messages: Vec<TimedMessage>,
    receiver: Option<UnboundedReceiver<(Duration, Message)>>,
}

impl LogState {
    pub fn new_with_channel() -> (Self, LogSender) {
        let (tx, rx) = unbounded_channel();
        (
            LogState {
                messages: Vec::new(),
                receiver: Some(rx),
            },
            LogSender { sender: tx },
        )
    }

    pub fn poll_messages(&mut self) {
        let now = SystemTime::now();

        if let Some(receiver) = &mut self.receiver {
            while let Ok((duration, message)) = receiver.try_recv() {
                self.messages.push(TimedMessage {
                    inserted: now,
                    duration,
                    message,
                });
            }
        }

        // Retain only messages that haven't expired
        self.messages
            .retain(|m| now.duration_since(m.inserted).unwrap_or_default() <= m.duration);
    }

    pub fn entries(&self) -> &[TimedMessage] {
        &self.messages
    }
}
