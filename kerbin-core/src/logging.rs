use std::time::{Duration, SystemTime};

use crate::*;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};
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

    let chunk_width = (window.size().width / 4) as usize;
    let notification_width = (chunk_width.saturating_sub(4)).min(60);
    let text_width = notification_width.saturating_sub(4);

    let mut total_height = 0u16;
    for msg in log.entries().iter() {
        let line_count = Paragraph::new(msg.message.message.as_str())
            .wrap(Wrap { trim: false })
            .line_count(text_width as u16);
        let notification_height = 2 + line_count as u16 + 1;
        total_height += notification_height + 1;
    }

    total_height = total_height.saturating_sub(1);

    let chunk_rect = Rect::new(
        window.size().width - window.size().width / 4,
        0,
        window.size().width / 4,
        total_height.min(window.size().height),
    );

    chunks.register_chunk::<LogChunk>(1, chunk_rect);
}

pub async fn render_log(log_chunk: Chunk<LogChunk>, log: ResMut<LogState>, theme: Res<Theme>) {
    let Some(mut chunk) = log_chunk.get().await else {
        return;
    };
    get!(mut log, theme);

    log.poll_messages();

    let area = chunk.area();
    let max_width = area.width as usize;

    let mut y_offset = 0u16;

    for msg in log.entries().iter() {
        let notification_height =
            render_notification(&mut chunk, msg, &theme, max_width, area, y_offset);

        y_offset += notification_height + 1;

        if y_offset >= area.height {
            break;
        }
    }
}

fn render_notification(
    buf: &mut ratatui::buffer::Buffer,
    msg: &TimedMessage,
    theme: &Theme,
    max_width: usize,
    chunk_area: Rect,
    top_y: u16,
) -> u16 {
    let style = msg.message.style(theme);
    let border_style = style;

    let margin = 2;
    let notification_width = (max_width.saturating_sub(margin * 2)).min(60) as u16;

    let text_width = notification_width.saturating_sub(4);
    let paragraph = Paragraph::new(msg.message.message.as_str())
        .style(style)
        .wrap(Wrap { trim: false });
    let line_count = paragraph.line_count(text_width);

    let start_y = chunk_area.y + top_y + 1;

    // Nothing to render if we're already past the bottom
    if start_y >= chunk_area.bottom() {
        return 2 + line_count as u16;
    }

    // Clamp height to remaining space so tall messages still render (truncated)
    let max_height = chunk_area.bottom() - start_y;
    let notification_height = (2 + line_count as u16).min(max_height);

    let start_x = chunk_area
        .width
        .saturating_sub(notification_width + margin as u16);

    let notification_rect = Rect::new(
        chunk_area.x + start_x,
        start_y,
        notification_width,
        notification_height,
    );

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Span::styled(format!(" [{}] ", msg.message.origin), style))
        .border_style(border_style);

    let inner = block.inner(notification_rect);

    block.render(notification_rect, buf);

    paragraph.render(inner, buf);

    notification_height
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
    pub fn style(&self, theme: &Theme) -> Style {
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

#[derive(State, Clone)]
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
            Duration::from_millis(4000),
            medium(origin, message),
        ));
        id
    }

    pub fn high(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(5000),
            high(origin, message),
        ));
        id
    }

    pub fn critical(&self, origin: impl ToString, message: impl ToString) -> MessageId {
        let id = self.next_id();
        let _ = self.sender.send(LogCommand::Add(
            id,
            Duration::from_millis(8000),
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
                            msg.inserted = now;
                        }
                    }
                    LogCommand::Remove(id) => {
                        self.messages.retain(|m| m.id != id);
                    }
                }
            }
        }

        self.messages
            .retain(|m| now.duration_since(m.inserted).unwrap_or_default() <= m.duration);
    }

    pub fn entries(&self) -> &[TimedMessage] {
        &self.messages
    }
}
