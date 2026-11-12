//! Split the persistent command-center rail from the active conversation.

use super::App;
use super::agents_overview::AGENTS_OVERVIEW_VIEW_ID;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Style;

#[derive(Clone, Copy)]
pub(super) struct AgentsOverviewDockLayout {
    pub(super) sidebar: Rect,
    pub(super) divider: Rect,
    pub(super) conversation: Rect,
    pub(super) command_divider: Rect,
    pub(super) command_reference: Rect,
}

impl AgentsOverviewDockLayout {
    pub(super) fn render_divider(&self, buf: &mut Buffer, sidebar_focused: bool) {
        let style = if sidebar_focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().dim()
        };
        for y in self.divider.y..self.divider.bottom() {
            buf[(self.divider.x, y)].set_symbol("|").set_style(style);
        }
        for y in self.command_divider.y..self.command_divider.bottom() {
            if !self.command_divider.is_empty() {
                buf[(self.command_divider.x, y)]
                    .set_symbol("|")
                    .set_style(Style::default().dim());
            }
        }
    }
}

pub(super) fn layout(viewport: Rect) -> Option<AgentsOverviewDockLayout> {
    if viewport.width < 3 {
        return None;
    }
    let sidebar_width = (viewport.width / 5)
        .clamp(12, 36)
        .min(viewport.width.saturating_sub(2));
    if viewport.width >= 120 {
        let command_width = (viewport.width / 5).clamp(28, 40);
        let [
            sidebar,
            divider,
            conversation,
            command_divider,
            command_reference,
        ] = Layout::horizontal([
            Constraint::Length(sidebar_width),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(command_width),
        ])
        .areas(viewport);
        Some(AgentsOverviewDockLayout {
            sidebar,
            divider,
            conversation,
            command_divider,
            command_reference,
        })
    } else {
        let [sidebar, divider, conversation] = Layout::horizontal([
            Constraint::Length(sidebar_width),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .areas(viewport);
        Some(AgentsOverviewDockLayout {
            sidebar,
            divider,
            conversation,
            command_divider: Rect::default(),
            command_reference: Rect::default(),
        })
    }
}

pub(super) fn conversation_viewport(app: &App, viewport: Rect) -> Rect {
    if app.conversation_frame.has_caller_inset()
        || app
            .chat_widget
            .selected_index_for_present_view(AGENTS_OVERVIEW_VIEW_ID)
            .is_some()
    {
        return viewport;
    }
    layout(viewport).map_or(viewport, |dock| dock.conversation)
}
