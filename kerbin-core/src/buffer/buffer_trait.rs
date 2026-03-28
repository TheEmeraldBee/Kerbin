use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::prelude::{Rect, StatefulWidget, Widget};
use std::any::Any;

use crate::{
    CoreConfig, CursorRenderState, GutterWidget, InnerChunk, SafeRopeAccess, TextBuffer,
    TextBufferWidget, Theme,
};

pub struct RenderContext<'a> {
    pub theme: &'a Theme,
    pub core_config: &'a CoreConfig,
}

pub trait KerbinBuffer: Send + Sync + 'static {
    /// Returns a `&dyn Any` for generic downcasting via `cur_buffer_as`
    fn as_any(&self) -> &dyn Any;

    /// Returns a `&mut dyn Any` for generic mutable downcasting via `cur_buffer_as_mut`
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Title shown in the bufferline tab
    fn title(&self) -> String;

    /// Whether the buffer has unsaved changes (shows '*' in tab)
    fn is_dirty(&self) -> bool {
        false
    }

    /// Render buffer content into the provided chunk area
    fn render(&mut self, area: Rect, chunk: &mut InnerChunk, focused: bool, ctx: &RenderContext);

    /// Render the left gutter (optional — default renders nothing)
    fn render_gutter(&self, _area: Rect, _chunk: &mut InnerChunk, _ctx: &RenderContext) {}

    /// Handle a key event while this buffer is focused.
    /// Return `true` if the event was consumed.
    fn handle_key(&mut self, _event: &KeyEvent) -> bool {
        false
    }

    /// Handle a mouse event directed at this buffer's area.
    /// `pos` is the (column, row) position relative to the buffer's chunk area.
    /// Return `true` if the event was consumed.
    fn handle_mouse(&mut self, _event: &MouseEvent, _pos: (u16, u16)) -> bool {
        false
    }

}

impl dyn KerbinBuffer {
    pub fn downcast<T: 'static>(&self) -> Option<&T> {
        self.as_any().downcast_ref()
    }

    pub fn downcast_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut()
    }
}

impl KerbinBuffer for TextBuffer {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn title(&self) -> String {
        self.path.clone()
    }

    fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn render(&mut self, area: Rect, chunk: &mut InnerChunk, focused: bool, ctx: &RenderContext) {
        let tab_style = ctx.theme.get_fallback_default(["ui.text.tabs", "ui.text"]);
        let cursor_on_tab_style = ctx.theme.get_fallback_default(["ui.selection"]);
        let mut cursor_state = CursorRenderState::default();
        TextBufferWidget::new(self)
            .with_vertical_scroll(self.renderer.byte_scroll)
            .with_horizontal_scroll(self.renderer.h_scroll)
            .with_tab_display_unit(ctx.core_config.tab_display_unit.clone())
            .with_tab_style(tab_style)
            .with_cursor_on_tab_style(cursor_on_tab_style)
            .render(area, chunk, &mut cursor_state);
        if focused {
            if let Some((cx, cy, shape)) = cursor_state.cursor {
                chunk.set_cursor(0, cx, cy, shape);
            } else {
                chunk.remove_cursor();
            }
        } else {
            chunk.remove_cursor();
        }
    }

    fn render_gutter(&self, area: Rect, chunk: &mut InnerChunk, ctx: &RenderContext) {
        GutterWidget::new(self.renderer.byte_scroll, self.len_lines(), ctx.theme)
            .render(area, chunk);
    }
}
