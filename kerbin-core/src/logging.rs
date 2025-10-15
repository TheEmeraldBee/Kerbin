use std::time::{Duration, SystemTime};

use crate::*;
use ascii_forge::{prelude::*, window::Render};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

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

    // Calculate total height needed for all notifications
    let max_width = window.size().x as usize;
    let notification_width = (max_width.saturating_sub(4)).min(60);
    let text_width = notification_width.saturating_sub(4);

    let mut total_height = 0u16;
    for msg in log.entries().iter() {
        let wrapped_lines = wrap_text(&msg.message.message, text_width);
        let notification_height = 2 + wrapped_lines.len() as u16 + 1;
        total_height += notification_height + 1; // +1 for spacing
    }

    // Remove trailing spacing
    total_height = total_height.saturating_sub(1);

    // Create chunk in top-right corner with exact size needed
    let chunk_rect = Rect::new(0, 0, window.size().x, total_height.min(window.size().y));

    chunks.register_chunk::<LogChunk>(1, chunk_rect);
}

pub async fn render_log(log_chunk: Chunk<LogChunk>, log: ResMut<LogState>, theme: Res<Theme>) {
    let Some(mut chunk) = log_chunk.get().await else {
        return;
    };
    get!(mut log, theme);

    log.poll_messages();

    let size = chunk.size();
    let max_width = size.x as usize;

    // Render notifications from top down, starting from top right
    let mut y_offset = 0;

    for msg in log.entries().iter() {
        let notification_height = render_notification(&mut chunk, msg, &theme, max_width, y_offset);

        y_offset += notification_height + 1; // +1 for spacing

        if y_offset >= size.y {
            break; // No more room
        }
    }
}

fn render_notification(
    chunk: &mut Buffer,
    msg: &TimedMessage,
    theme: &Theme,
    max_width: usize,
    top_y: u16,
) -> u16 {
    let style = msg.message.style(theme);
    let border_style = style;

    // Calculate notification width (leave some margin from edges)
    let margin = 2;
    let notification_width = (max_width.saturating_sub(margin * 2)).min(60);

    // Wrap the message text
    let text_width = notification_width.saturating_sub(4); // Account for borders and padding
    let wrapped_lines = wrap_text(&msg.message.message, text_width);

    // Calculate height: top border + namespace + wrapped lines + bottom border
    let notification_height = 2 + wrapped_lines.len() as u16 + 1;

    // Calculate x position (right-aligned with margin)
    let start_x = chunk
        .size()
        .x
        .saturating_sub(notification_width as u16 + margin as u16);

    // Render top border with rounded corners
    let top_border = format!("╭{}╮", "─".repeat(notification_width.saturating_sub(2)));
    render!(chunk, vec2(start_x, top_y) => [border_style.apply(&top_border)]);

    // Render namespace line
    let namespace_text = format!(" [{}] ", msg.message.origin);
    let namespace_line = format!(
        "│{}{}│",
        namespace_text,
        " ".repeat(
            notification_width
                .saturating_sub(2)
                .saturating_sub(namespace_text.len())
        )
    );
    render!(chunk, vec2(start_x, top_y + 1) => [border_style.apply(&namespace_line)]);

    // Render wrapped message lines
    for (i, line) in wrapped_lines.iter().enumerate() {
        let padded_line = format!(
            "│ {}{} │",
            line,
            " ".repeat(text_width.saturating_sub(line.len()))
        );
        render!(chunk, vec2(start_x, top_y + 2 + i as u16) => [style.apply(&padded_line)]);
    }

    // Render bottom border with rounded corners
    let bottom_border = format!("╰{}╯", "─".repeat(notification_width.saturating_sub(2)));
    render!(chunk, vec2(start_x, top_y + notification_height - 1) => [border_style.apply(&bottom_border)]);

    notification_height
}

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MessageId(u64);

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

enum LogCommand {
    Add(MessageId, Duration, Message),
    Modify(MessageId, String),
    Remove(MessageId),
}

#[derive(State)]
pub struct LogSender {
    sender: UnboundedSender<LogCommand>,
    next_id: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl LogSender {
    fn next_id(&self) -> MessageId {
        MessageId(
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        )
    }

    pub fn low(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(3000),
            low(origin, message),
        ));
        id
    }

    pub fn medium(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(5000),
            medium(origin, message),
        ));
        id
    }

    pub fn high(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(8000),
            high(origin, message),
        ));
        id
    }

    pub fn critical(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(10000),
            critical(origin, message),
        ));
        id
    }

    pub fn modify(&self, id: MessageId, new_message: impl ToString) {
        let _ = self
            .sender
            .send(LogCommand::Modify(id, new_message.to_string()));
    }

    pub fn remove(&self, id: MessageId) {
        let _ = self.sender.send(LogCommand::Remove(id));
    }
}

pub struct TimedMessage {
    pub id: MessageId,
    pub inserted: SystemTime,
    pub duration: Duration,
    pub message: Message,
}

#[derive(State, Default)]
pub struct LogState {
    messages: Vec<TimedMessage>,
    receiver: Option<UnboundedReceiver<LogCommand>>,
}

impl LogState {
    pub fn new_with_channel() -> (Self, LogSender) {
        let (tx, rx) = unbounded_channel();
        (
            LogState {
                messages: Vec::new(),
                receiver: Some(rx),
            },
            LogSender {
                sender: tx,
                next_id: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            },
        )
    }

    pub fn poll_messages(&mut self) {
        let now = SystemTime::now();

        if let Some(receiver) = &mut self.receiver {
            while let Ok(command) = receiver.try_recv() {
                match command {
                    LogCommand::Add(id, duration, message) => {
                        self.messages.push(TimedMessage {
                            id,
                            inserted: now,
                            duration,
                            message,
                        });
                    }
                    LogCommand::Modify(id, new_message) => {
                        if let Some(msg) = self.messages.iter_mut().find(|m| m.id == id) {
                            msg.message.message = new_message;
                            // Reset the timer when modified
                            msg.inserted = now;
                        }
                    }
                    LogCommand::Remove(id) => {
                        self.messages.retain(|m| m.id != id);
                    }
                }
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
