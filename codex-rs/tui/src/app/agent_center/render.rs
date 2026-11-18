//! Status tabs, task rows, the rename editor and details share a wide/narrow layout.

use super::hints::hint_line;
use super::*;
use crate::bottom_pane::render_filled_tab_bar;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crossterm::cursor::SetCursorStyle;
use ratatui::widgets::WidgetRef;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn row(area: Rect, offset: u16, height: u16) -> Rect {
    let offset = offset.min(area.height);
    Rect::new(
        area.x,
        area.y + offset,
        area.width,
        height.min(area.height - offset),
    )
}

pub(super) fn line(text: impl Into<Line<'static>>, area: Rect, buf: &mut Buffer) {
    if !area.is_empty() {
        truncate_line_with_ellipsis_if_overflow(text.into(), usize::from(area.width))
            .render(row(area, /*offset*/ 0, /*height*/ 1), buf);
    }
}

const MIN_DOCKED_LIST_HEIGHT: u16 = 12;
const MIN_DOCKED_DETAILS_HEIGHT: u16 = 8;

fn visible_suffix(input: &str, available: usize) -> &str {
    let mut start = input.len();
    let mut visible_width = 0;
    for (offset, grapheme) in input.grapheme_indices(/*is_extended*/ true).rev() {
        let width = grapheme.width();
        if visible_width + width > available {
            break;
        }
        visible_width += width;
        start = offset;
    }
    &input[start..]
}

struct CenterLayout {
    header: Rect,
    list: Rect,
    gap: Rect,
    details: Rect,
    search: Rect,
    footer: Rect,
}

impl AgentsOverviewView {
    pub(in crate::app) fn clamp_docked_details_height(
        requested: u16,
        area_height: u16,
    ) -> u16 {
        let maximum = area_height
            .saturating_sub(MIN_DOCKED_LIST_HEIGHT + 1)
            .max(MIN_DOCKED_DETAILS_HEIGHT);
        requested.clamp(MIN_DOCKED_DETAILS_HEIGHT, maximum)
    }

    fn center_layout(&self, area: Rect) -> CenterLayout {
        let state = self.state();
        let footer_height = u16::from(area.height >= 2);
        let header_height = if area.height >= 6 { 3 } else { 0 };
        let footer = row(area, area.height - footer_height, footer_height);
        let header = row(area, /*offset*/ 0, header_height);
        let body = row(area, header_height, footer.y - header.bottom())
            .inner(Margin::new(/*horizontal*/ 2, /*vertical*/ 0));
        let stacked_details = area.width < 90 && area.height >= 26;
        let [list, gap, details] = if stacked_details {
            let default_height = (u32::from(body.height) * 2 / 5).clamp(12, 26) as u16;
            let details_height = Self::clamp_docked_details_height(
                state.docked_details_height.unwrap_or(default_height),
                body.height,
            );
            let list_height = body.height.saturating_sub(details_height + 1);
            Layout::vertical([
                Constraint::Length(list_height),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .areas(body)
        } else if body.width >= 90 {
            Layout::horizontal([
                Constraint::Min(46),
                Constraint::Length(3),
                Constraint::Length(38),
            ])
            .areas(body)
        } else {
            [body, Rect::default(), Rect::default()]
        };
        let search = row(list, /*offset*/ 0, u16::from(state.editing_metadata()));
        let list = row(
            list,
            search.height,
            list.height.saturating_sub(search.height),
        );
        CenterLayout {
            header,
            list,
            gap,
            details,
            search,
            footer,
        }
    }
}

impl Renderable for AgentsOverviewView {
    fn desired_height(&self, _width: u16) -> u16 {
        26
    }

    fn cursor_style(&self, _area: Rect) -> SetCursorStyle {
        let state = self.state();
        if state.rename_target.is_some() && state.input.uses_vim_insert_cursor() {
            SetCursorStyle::SteadyBar
        } else {
            SetCursorStyle::DefaultUserShape
        }
    }

    fn cursor_pos(&self, area: Rect) -> Option<(u16, u16)> {
        let layout = self.center_layout(area);
        let state = self.state();
        if state.editing_metadata() && !layout.search.is_empty() {
            if state.rename_target.is_some() {
                let mut area = layout.search;
                let prefix_width = 9.min(area.width);
                area.x += prefix_width;
                area.width -= prefix_width;
                return state.input.cursor_pos(area);
            }
            let input = &state.search;
            let prefix = "Search › ";
            let available = usize::from(layout.search.width).saturating_sub(prefix.width() + 1);
            return Some((
                layout.search.x
                    + (prefix.width() + visible_suffix(input, available).width())
                        .min(usize::from(layout.search.width.saturating_sub(/*rhs*/ 1)))
                        as u16,
                layout.search.y,
            ));
        }
        None
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let layout = self.center_layout(area);
        let inset =
            |area: Rect| area.inner(Margin::new(/*horizontal*/ 2, /*vertical*/ 0));
        let footer_hints = self.center_footer_hints();
        let mut state = self.state();
        let stacked_details = layout.gap.height == 1 && layout.gap.width > 1;
        state.details_divider_area = stacked_details.then_some(layout.gap);
        state.docked_area_height = layout.list.height + layout.gap.height + layout.details.height;
        if stacked_details {
            state.docked_details_height = Some(layout.details.height);
        } else {
            state.docked_details_resizing = false;
        }
        let resizing_details = state.docked_details_resizing;
        let filter_keys = if state.editing_metadata() {
            String::new()
        } else {
            self.center_filter_hint()
        };
        let grouping = match state.grouping {
            AgentsOverviewGrouping::Project => "Project",
            AgentsOverviewGrouping::Directory => "Directory",
            AgentsOverviewGrouping::Status => "Status",
            AgentsOverviewGrouping::Model => "Model",
        };
        let group_key = self
            .agents_keymap
            .primary_hint("toggle_grouping", &self.agents_keymap.toggle_grouping)
            .map(ShortcutHint::display_label)
            .unwrap_or_default();
        line(
            vec![
                "Agent command center".bold(),
                format!("  Group: {grouping}  {group_key}").dim(),
            ],
            inset(layout.header),
            buf,
        );
        let history = state.history.as_ref();
        let labels = TASK_FILTERS
            .iter()
            .map(|(label, group)| {
                let count = self
                    .rows
                    .iter()
                    .filter(|row| state.includes_row(row))
                    .filter(|row| group.is_none_or(|group| group == row.group))
                    .count();
                format!("{label} {count}")
            })
            .collect::<Vec<_>>();
        let mut tabs = inset(row(layout.header, /*offset*/ 1, /*height*/ 1));
        let filter_hint = hint_line(&[(filter_keys, "filter".into())]);
        if tabs.width >= 80 && filter_hint.width() > 0 {
            let hint_width = filter_hint.width() as u16;
            filter_hint.render(
                Rect::new(tabs.right() - hint_width, tabs.y, hint_width, tabs.height),
                buf,
            );
            tabs.width = tabs.width.saturating_sub(hint_width + 2);
        }
        render_filled_tab_bar(
            &labels.iter().map(String::as_str).collect::<Vec<_>>(),
            state.status_filter,
            tabs,
            buf,
        );
        line(
            "─"
                .repeat(usize::from(area.width.saturating_sub(/*rhs*/ 4)))
                .dim(),
            inset(row(layout.header, /*offset*/ 2, /*height*/ 1)),
            buf,
        );
        if state.editing_metadata() {
            buf.set_style(layout.search, crate::bottom_pane::active_tab_style());
            if state.rename_target.is_some() {
                line("Rename › ".cyan().bold(), layout.search, buf);
                let mut area = layout.search;
                let prefix_width = 9.min(area.width);
                area.x += prefix_width;
                area.width -= prefix_width;
                WidgetRef::render_ref(&&state.input, area, buf);
            } else {
                let label = "Search › ";
                let available = usize::from(layout.search.width).saturating_sub(label.width() + 1);
                line(
                    vec![
                        label.cyan().bold(),
                        visible_suffix(&state.search, available).to_owned().into(),
                    ],
                    layout.search,
                    buf,
                );
            }
        }
        let notice = state
            .connection_notice
            .map(str::to_owned)
            .or_else(|| {
                state
                    .creating_worktree
                    .then(|| "Creating worktree…".to_owned())
            })
            .or_else(|| {
                state.removal.map(|action| match action {
                    AgentsOverviewAction::Archive => {
                        "Archiving task…  You can still browse.".to_owned()
                    }
                    AgentsOverviewAction::Delete => {
                        "Deleting task…  You can still browse.".to_owned()
                    }
                })
            })
            .or_else(|| {
                let failed = state.refresh_failed || history.is_some_and(|s| s.failed);
                (failed && !state.loading).then(|| "Error loading tasks".to_owned())
            })
            .or_else(|| state.server_version_notice.clone());
        if let Some(notice) = notice.filter(|_| !state.help) {
            line(notice.dim(), inset(layout.footer), buf);
        } else {
            line(hint_line(&footer_hints), inset(layout.footer), buf);
        }
        let help = state.help;
        drop(state);
        if help {
            let body = row(
                area,
                layout.header.height,
                layout.footer.y - layout.header.bottom(),
            );
            Clear.render(inset(body), buf);
            let body = inset(body);
            let mut lines = self.center_help_lines(body.width);
            if lines.len() > usize::from(body.height) {
                lines.truncate(usize::from(body.height.saturating_sub(/*rhs*/ 1)));
                lines.push("… resize to see all".dim().into());
            }
            Paragraph::new(lines).render(body, buf);
            return;
        }
        if layout.gap.height == 1 && layout.gap.width > 1 {
            let divider_style = if resizing_details {
                ratatui::style::Style::default()
                    .fg(ratatui::style::Color::Cyan)
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                ratatui::style::Style::default().dim()
            };
            for x in layout.gap.x..layout.gap.right() {
                buf[(x, layout.gap.y)]
                    .set_symbol("-")
                    .set_style(divider_style);
            }
        } else {
            for y in layout.gap.y..layout.gap.bottom() {
                if layout.gap.width > 1 {
                    buf[(layout.gap.x + 1, y)]
                        .set_symbol("\u{2502}")
                        .set_style(Style::default().dim());
                }
            }
        }
        self.render_center_rows(layout.list, buf);
        if !layout.details.is_empty() {
            self.render_details(layout.details, buf);
        }
    }
}
