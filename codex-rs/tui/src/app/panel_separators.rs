use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

#[derive(Clone, Copy)]
pub(super) enum VerticalJunction {
    FromLeft,
}

pub(super) fn draw_horizontal(buf: &mut Buffer, area: Rect, style: Style) {
    if area.height == 0 {
        return;
    }
    for x in area.x..area.right() {
        buf[(x, area.y)]
            .set_symbol("\u{2500}")
            .set_style(style);
    }
}

pub(super) fn draw_vertical(
    buf: &mut Buffer,
    area: Rect,
    style: Style,
    junctions: &[(u16, VerticalJunction)],
) {
    if area.width == 0 {
        return;
    }
    for y in area.y..area.bottom() {
        let junction = junctions
            .iter()
            .find(|(row, _)| *row == y)
            .map(|(_, junction)| *junction);
        draw_vertical_at(buf, area.x, y, style, junction);
    }
}

pub(super) fn draw_vertical_at(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    style: Style,
    junction: Option<VerticalJunction>,
) {
    let symbol = match junction {
        Some(VerticalJunction::FromLeft) => "\u{2524}",
        None => "\u{2502}",
    };
    buf[(x, y)].set_symbol(symbol).set_style(style);
}
