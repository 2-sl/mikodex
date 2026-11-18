use ratatui::buffer::Buffer;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Clear;
use ratatui::widgets::Widget;

fn content_area(area: Rect) -> Rect {
    let inner = area.inner(Margin::new(/*horizontal*/ 1, /*vertical*/ 0));
    Rect::new(
        inner.x,
        inner.y.saturating_add(3).min(inner.bottom()),
        inner.width,
        inner.height.saturating_sub(4),
    )
}

fn entries(items: &[(String, String)], width: u16) -> Vec<Vec<Line<'static>>> {
    items
        .iter()
        .map(|(command, description)| {
            let mut lines = vec![Line::from(format!("/{command}").cyan().bold())];
            if !description.is_empty() && width > 2 {
                lines.extend(crate::wrapping::word_wrap_lines(
                    [Line::from(format!("  {description}").dim())],
                    usize::from(width),
                ));
            }
            lines
        })
        .collect()
}

pub(super) fn max_scroll(items: &[(String, String)], area: Rect) -> usize {
    let content = content_area(area);
    let entries = entries(items, content.width);
    let mut height = 0_u16;
    let mut first_visible = entries.len();
    while first_visible > 0 && height < content.height {
        first_visible -= 1;
        height = height.saturating_add(entries[first_visible].len() as u16);
    }
    first_visible
}

pub(super) fn render(area: Rect, buf: &mut Buffer, items: &[(String, String)], scroll: usize) {
    Clear.render(area, buf);
    let inner = area.inner(Margin::new(/*horizontal*/ 1, /*vertical*/ 0));
    if inner.is_empty() {
        return;
    }
    let title = Line::from(vec![
        "Slash commands".cyan().bold(),
        format!("  {}", items.len()).dim(),
    ]);
    title.render(
        Rect::new(inner.x, inner.y, inner.width, 1.min(inner.height)),
        buf,
    );
    if inner.height > 1 {
        Line::from("Type / in chat to run one".dim())
            .render(Rect::new(inner.x, inner.y + 1, inner.width, 1), buf);
    }
    if inner.height > 2 {
        Line::from("-".repeat(usize::from(inner.width)).dim())
            .render(Rect::new(inner.x, inner.y + 2, inner.width, 1), buf);
    }
    let content = content_area(area);
    let entries = entries(items, content.width);
    let scroll = scroll.min(max_scroll(items, area));
    let mut y = content.y;
    for entry in entries.iter().skip(scroll) {
        for line in entry {
            if y >= content.bottom() {
                break;
            }
            line.clone()
                .render(Rect::new(content.x, y, content.width, 1), buf);
            y += 1;
        }
        if y >= content.bottom() {
            break;
        }
    }
    if inner.height > 3 {
        Line::from("Wheel to browse".dim())
            .render(Rect::new(inner.x, inner.bottom() - 1, inner.width, 1), buf);
    }
}
