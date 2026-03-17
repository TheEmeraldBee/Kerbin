use ratatui::{prelude::*, widgets::Paragraph};

use crate::Theme;

/// Widget that renders line numbers into a gutter area
pub struct GutterWidget {
    line_scroll: usize,
    total_lines: usize,
    style: Style,
}

impl GutterWidget {
    pub fn new(line_scroll: usize, total_lines: usize, theme: &Theme) -> Self {
        Self {
            line_scroll,
            total_lines,
            style: theme.get_fallback_default(["ui.gutter"]),
        }
    }
}

impl Widget for GutterWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width as usize;
        let lines: Vec<Line<'static>> = (0..area.height)
            .map(|row| {
                let line_num = self.line_scroll + row as usize + 1;
                if line_num > self.total_lines {
                    Line::default()
                } else {
                    Line::raw(format!("{:>width$}", line_num))
                }
            })
            .collect();
        Paragraph::new(Text::from(lines))
            .style(self.style)
            .render(area, buf);
    }
}
