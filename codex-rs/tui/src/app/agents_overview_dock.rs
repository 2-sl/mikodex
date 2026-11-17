//! Split the persistent command-center rail from the active conversation.

use super::App;
use super::agents_overview::AGENTS_OVERVIEW_VIEW_ID;
use super::agents_overview_view::AgentsOverviewView;
use crate::key_hint::KeyBindingListExt;
use crate::legacy_core::config::edit::ConfigEdit;
use crate::legacy_core::config::edit::ConfigEditsBuilder;
use crate::tui::Tui;
use crate::tui::TuiEvent;
use color_eyre::eyre::Result;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use crossterm::event::KeyModifiers;
use crossterm::event::MouseButton;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Constraint;
use ratatui::layout::Layout;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;

const MIN_LEFT_WIDTH: u16 = 12;
const MIN_RIGHT_WIDTH: u16 = 24;
const MAX_DOCK_WIDTH: u16 = 60;
const MIN_CONVERSATION_WIDTH: u16 = 32;
pub(super) const RESIZE_HINT: &str =
    "Dock width · ←/→ · Shift+←/→ 10 cols · Enter apply · Esc cancel";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DockDivider {
    AgentsOverview,
    CommandReference,
}

#[derive(Clone, Copy)]
pub(super) struct DockResizeMode {
    pub(super) divider: DockDivider,
    pub(super) preview_width: u16,
}

#[derive(Clone, Copy)]
pub(super) struct DockDividerDrag {
    pub(super) divider: DockDivider,
    press: u16,
    initial_width: u16,
    pub(super) width: u16,
    screen: Rect,
}

#[derive(Clone, Copy)]
pub(super) struct DetailsDividerDrag {
    press_y: u16,
    initial_height: u16,
    screen: Rect,
}

#[derive(Clone, Copy)]
pub(super) struct AgentsOverviewDockLayout {
    pub(super) sidebar: Rect,
    pub(super) divider: Rect,
    pub(super) conversation: Rect,
    pub(super) command_divider: Rect,
    pub(super) command_reference: Rect,
}

impl AgentsOverviewDockLayout {
    pub(super) fn render_divider(&self, buf: &mut Buffer, app: &App) {
        let left_style = if app.agents_overview.sidebar_focused
            || app
                .agents_overview_dock_resize
                .is_some_and(|mode| mode.divider == DockDivider::AgentsOverview)
            || app
                .agents_overview_dock_drag
                .is_some_and(|drag| drag.divider == DockDivider::AgentsOverview)
        {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().dim()
        };
        for y in self.divider.y..self.divider.bottom() {
            buf[(self.divider.x, y)]
                .set_symbol("|")
                .set_style(left_style);
        }
        let right_style = if app.agents_overview.command_reference_focused
            || app
                .agents_overview_dock_resize
                .is_some_and(|mode| mode.divider == DockDivider::CommandReference)
            || app
                .agents_overview_dock_drag
                .is_some_and(|drag| drag.divider == DockDivider::CommandReference)
        {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().dim()
        };
        for y in self.command_divider.y..self.command_divider.bottom() {
            if !self.command_divider.is_empty() {
                buf[(self.command_divider.x, y)]
                    .set_symbol("|")
                    .set_style(right_style);
            }
        }
    }
}

pub(super) fn layout(app: &App, viewport: Rect) -> Option<AgentsOverviewDockLayout> {
    if viewport.width < 3 {
        return None;
    }
    let has_command_reference = viewport.width >= 120;
    let default_left = (viewport.width / 5).clamp(MIN_LEFT_WIDTH, 36);
    let default_right = (viewport.width / 5).clamp(MIN_RIGHT_WIDTH, 40);
    let right_minimum = if has_command_reference {
        MIN_RIGHT_WIDTH
    } else {
        0
    };
    let left_maximum = viewport
        .width
        .saturating_sub(2 + MIN_CONVERSATION_WIDTH + right_minimum)
        .min(MAX_DOCK_WIDTH)
        .max(1);
    let mut left_width = configured_width(
        viewport.width,
        app.local_settings.tui.agents_overview_width_percent,
        default_left,
        MIN_LEFT_WIDTH,
        left_maximum,
    );
    let left_preview = app
        .agents_overview_dock_drag
        .filter(|drag| drag.divider == DockDivider::AgentsOverview)
        .map(|drag| drag.width)
        .or_else(|| {
            app.agents_overview_dock_resize
                .filter(|mode| mode.divider == DockDivider::AgentsOverview)
                .map(|mode| mode.preview_width)
        });
    if let Some(width) = left_preview {
        left_width = clamp_dock_width(width, MIN_LEFT_WIDTH, left_maximum);
    }

    if has_command_reference {
        let right_maximum = viewport
            .width
            .saturating_sub(left_width + 2 + MIN_CONVERSATION_WIDTH)
            .min(MAX_DOCK_WIDTH)
            .max(1);
        let mut right_width = configured_width(
            viewport.width,
            app.local_settings.tui.command_reference_width_percent,
            default_right,
            MIN_RIGHT_WIDTH,
            right_maximum,
        );
        let right_preview = app
            .agents_overview_dock_drag
            .filter(|drag| drag.divider == DockDivider::CommandReference)
            .map(|drag| drag.width)
            .or_else(|| {
                app.agents_overview_dock_resize
                    .filter(|mode| mode.divider == DockDivider::CommandReference)
                    .map(|mode| mode.preview_width)
            });
        if let Some(width) = right_preview {
            right_width = clamp_dock_width(width, MIN_RIGHT_WIDTH, right_maximum);
        }
        let [
            sidebar,
            divider,
            conversation,
            command_divider,
            command_reference,
        ] = Layout::horizontal([
            Constraint::Length(left_width),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Length(right_width),
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
            Constraint::Length(left_width),
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

fn configured_width(total: u16, percent: Option<u8>, default: u16, min: u16, max: u16) -> u16 {
    let desired = percent.map_or(default, |percent| {
        ((u32::from(total) * u32::from(percent) + 50) / 100) as u16
    });
    clamp_dock_width(desired, min, max)
}

fn clamp_dock_width(width: u16, min: u16, max: u16) -> u16 {
    width.clamp(min.min(max.max(1)), max.max(1))
}

fn width_percent(width: u16, total: u16) -> u8 {
    ((u32::from(width) * 100 + u32::from(total) / 2) / u32::from(total).max(1)).clamp(1, 99) as u8
}

fn divider_maximum(divider: DockDivider, viewport: Rect, other_width: u16) -> u16 {
    match divider {
        DockDivider::AgentsOverview if viewport.width >= 120 => viewport
            .width
            .saturating_sub(other_width + 2 + MIN_CONVERSATION_WIDTH)
            .min(MAX_DOCK_WIDTH)
            .max(1),
        DockDivider::AgentsOverview => viewport.width.saturating_sub(2).min(MAX_DOCK_WIDTH).max(1),
        DockDivider::CommandReference => viewport
            .width
            .saturating_sub(other_width + 2 + MIN_CONVERSATION_WIDTH)
            .min(MAX_DOCK_WIDTH)
            .max(1),
    }
}

fn divider_minimum(divider: DockDivider) -> u16 {
    match divider {
        DockDivider::AgentsOverview => MIN_LEFT_WIDTH,
        DockDivider::CommandReference => MIN_RIGHT_WIDTH,
    }
}

fn conversation_area_visible(app: &App) -> bool {
    !app.conversation_frame.has_caller_inset()
        && app
            .chat_widget
            .selected_index_for_present_view(AGENTS_OVERVIEW_VIEW_ID)
            .is_none()
}

pub(super) fn conversation_viewport(app: &App, viewport: Rect) -> Rect {
    if !conversation_area_visible(app) {
        return viewport;
    }
    layout(app, viewport).map_or(viewport, |dock| dock.conversation)
}

impl App {
    pub(super) async fn handle_agents_overview_dock_resize(
        &mut self,
        tui: &mut Tui,
        event: &TuiEvent,
    ) -> Result<bool> {
        if let Some(mode) = self.agents_overview_dock_resize {
            match event {
                TuiEvent::Key(key) if key.kind == KeyEventKind::Release => return Ok(true),
                TuiEvent::Key(key) => {
                    let viewport = tui.prepare_draw_size()?.into();
                    let Some(dock) = layout(self, viewport) else {
                        self.agents_overview_dock_resize = None;
                        tui.frame_requester().schedule_frame();
                        return Ok(false);
                    };
                    let current_width = match mode.divider {
                        DockDivider::AgentsOverview => dock.sidebar.width,
                        DockDivider::CommandReference => dock.command_reference.width,
                    };
                    let step = match (key.code, key.modifiers) {
                        (KeyCode::Left | KeyCode::Char('h'), KeyModifiers::NONE) => -1,
                        (KeyCode::Right | KeyCode::Char('l'), KeyModifiers::NONE) => 1,
                        (KeyCode::Left | KeyCode::Char('h'), KeyModifiers::SHIFT) => -10,
                        (KeyCode::Right | KeyCode::Char('l'), KeyModifiers::SHIFT) => 10,
                        (KeyCode::Enter, KeyModifiers::NONE) if key.kind == KeyEventKind::Press => {
                            self.agents_overview_dock_resize = None;
                            self.save_agents_overview_dock_width(
                                mode.divider,
                                current_width,
                                viewport.width,
                            )
                            .await;
                            tui.frame_requester().schedule_frame();
                            return Ok(true);
                        }
                        (KeyCode::Esc, KeyModifiers::NONE) if key.kind == KeyEventKind::Press => {
                            self.agents_overview_dock_resize = None;
                            tui.frame_requester().schedule_frame();
                            return Ok(true);
                        }
                        _ => return Ok(true),
                    };
                    let direction = match mode.divider {
                        DockDivider::AgentsOverview => step,
                        DockDivider::CommandReference => -step,
                    };
                    let other_width = match mode.divider {
                        DockDivider::AgentsOverview => dock.command_reference.width,
                        DockDivider::CommandReference => dock.sidebar.width,
                    };
                    let min = divider_minimum(mode.divider);
                    let max = divider_maximum(mode.divider, viewport, other_width);
                    let next = (i32::from(current_width) + direction)
                        .clamp(i32::from(min.min(max)), i32::from(max.max(1)))
                        as u16;
                    if next != current_width {
                        self.agents_overview_dock_resize = Some(DockResizeMode {
                            divider: mode.divider,
                            preview_width: next,
                        });
                        tui.frame_requester().schedule_frame();
                    }
                    Ok(true)
                }
                TuiEvent::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                    self.agents_overview_dock_resize = None;
                    tui.frame_requester().schedule_frame();
                    self.handle_agents_overview_dock_mouse(tui, mouse).await
                }
                TuiEvent::Resize(_) | TuiEvent::FocusLost => {
                    self.agents_overview_dock_resize = None;
                    tui.frame_requester().schedule_frame();
                    Ok(false)
                }
                _ => Ok(true),
            }
        } else if let TuiEvent::Mouse(mouse) = event {
            self.handle_agents_overview_dock_mouse(tui, mouse).await
        } else if matches!(event, TuiEvent::Resize(_) | TuiEvent::FocusLost)
            && self.agents_overview.details_divider_drag.take().is_some()
        {
            if let Some(view) = self.agents_overview.sidebar_view.as_ref() {
                view.set_docked_details_resizing(false);
            }
            tui.frame_requester().schedule_frame();
            Ok(false)
        } else if let TuiEvent::Key(key) = event
            && key.kind == KeyEventKind::Press
            && (self.agents_overview.sidebar_focused || !self.can_resize_sidebar(tui))
            && self.keymap.app.resize_panes.is_pressed(*key)
            && self.overlay.is_none()
            && !self.chat_widget.has_active_modal()
            && conversation_area_visible(self)
        {
            let viewport = tui.prepare_draw_size()?.into();
            let Some(dock) = layout(self, viewport) else {
                return Ok(false);
            };
            let divider =
                if self.agents_overview.sidebar_focused || dock.command_reference.is_empty() {
                    DockDivider::AgentsOverview
                } else {
                    DockDivider::CommandReference
                };
            let preview_width = match divider {
                DockDivider::AgentsOverview => dock.sidebar.width,
                DockDivider::CommandReference => dock.command_reference.width,
            };
            self.agents_overview_dock_resize = Some(DockResizeMode {
                divider,
                preview_width,
            });
            self.divider_drag = None;
            self.cancel_pending_key_chord();
            tui.frame_requester().schedule_frame();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn handle_agents_overview_dock_mouse(
        &mut self,
        tui: &mut Tui,
        mouse: &crossterm::event::MouseEvent,
    ) -> Result<bool> {
        if !conversation_area_visible(self)
            || self.overlay.is_some()
            || self.chat_widget.has_active_modal()
        {
            let dock_drag_cleared = self.agents_overview_dock_drag.take().is_some();
            let details_drag_cleared =
                self.agents_overview.details_divider_drag.take().is_some();
            if details_drag_cleared
                && let Some(view) = self.agents_overview.sidebar_view.as_ref()
            {
                view.set_docked_details_resizing(false);
            }
            if dock_drag_cleared || details_drag_cleared {
                tui.frame_requester().schedule_frame();
            }
            return Ok(false);
        }
        if let Some(drag) = self.agents_overview.details_divider_drag {
            let viewport: Rect = tui.prepare_draw_size()?.into();
            if viewport != drag.screen {
                self.agents_overview.details_divider_drag = None;
                if let Some(view) = self.agents_overview.sidebar_view.as_ref() {
                    view.set_docked_details_resizing(false);
                }
                tui.frame_requester().schedule_frame();
                return Ok(false);
            }
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left) => {
                    let delta_y = i32::from(mouse.row) - i32::from(drag.press_y);
                    let requested_height = (i32::from(drag.initial_height) - delta_y)
                        .clamp(0, i32::from(u16::MAX)) as u16;
                    let details_height =
                        AgentsOverviewView::clamp_docked_details_height(
                            requested_height,
                            viewport.height,
                        );
                    if let Some(view) = self.agents_overview.sidebar_view.as_ref() {
                        view.set_docked_details_height(details_height);
                        if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                            view.set_docked_details_resizing(false);
                        }
                    }
                    if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                        self.agents_overview.details_divider_drag = None;
                    } else {
                        self.agents_overview.details_divider_drag = Some(drag);
                    }
                    tui.frame_requester().schedule_frame();
                    return Ok(true);
                }
                _ => {
                    self.agents_overview.details_divider_drag = None;
                    if let Some(view) = self.agents_overview.sidebar_view.as_ref() {
                        view.set_docked_details_resizing(false);
                    }
                    tui.frame_requester().schedule_frame();
                    return Ok(false);
                }
            }
        }
        if let Some(mut drag) = self.agents_overview_dock_drag {
            let viewport = tui.prepare_draw_size()?.into();
            if viewport != drag.screen {
                self.agents_overview_dock_drag = None;
                tui.frame_requester().schedule_frame();
                return Ok(false);
            }
            match mouse.kind {
                MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                    let delta = i32::from(mouse.column) - i32::from(drag.press);
                    let signed_delta = match drag.divider {
                        DockDivider::AgentsOverview => delta,
                        DockDivider::CommandReference => -delta,
                    };
                    let other_width = layout(self, viewport).map_or(0, |dock| match drag.divider {
                        DockDivider::AgentsOverview => dock.command_reference.width,
                        DockDivider::CommandReference => dock.sidebar.width,
                    });
                    let min = divider_minimum(drag.divider);
                    let max = divider_maximum(drag.divider, viewport, other_width);
                    drag.width = clamp_dock_width(
                        (i32::from(drag.initial_width) + signed_delta).clamp(0, i32::from(u16::MAX))
                            as u16,
                        min,
                        max,
                    );
                    if mouse.kind == MouseEventKind::Up(MouseButton::Left) {
                        self.agents_overview_dock_drag = None;
                        self.save_agents_overview_dock_width(
                            drag.divider,
                            drag.width,
                            viewport.width,
                        )
                        .await;
                    } else {
                        self.agents_overview_dock_drag = Some(drag);
                    }
                    tui.frame_requester().schedule_frame();
                    return Ok(true);
                }
                _ => {
                    self.agents_overview_dock_drag = None;
                    tui.frame_requester().schedule_frame();
                    return Ok(false);
                }
            }
        }

        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let viewport = tui.prepare_draw_size()?.into();
            let Some(dock) = layout(self, viewport) else {
                return Ok(false);
            };
            let point = (mouse.column, mouse.row).into();
            if let Some(view) = self.agents_overview.sidebar_view.as_ref()
                && let Some((divider, initial_height)) =
                    view.docked_details_divider(dock.sidebar)
                && divider.contains(point)
            {
                self.agents_overview.details_divider_drag =
                    Some(DetailsDividerDrag {
                        press_y: mouse.row,
                        initial_height,
                        screen: viewport,
                    });
                view.set_docked_details_resizing(true);
                self.agents_overview_dock_drag = None;
                self.pane_resize = None;
                self.divider_drag = None;
                self.cancel_pending_key_chord();
                tui.frame_requester().schedule_frame();
                return Ok(true);
            }
            let (divider, initial_width) = if dock.divider.contains(point) {
                (DockDivider::AgentsOverview, dock.sidebar.width)
            } else if dock.command_divider.contains(point) && !dock.command_divider.is_empty() {
                (DockDivider::CommandReference, dock.command_reference.width)
            } else {
                return Ok(false);
            };
            self.agents_overview_dock_drag = Some(DockDividerDrag {
                divider,
                press: mouse.column,
                initial_width,
                width: initial_width,
                screen: viewport,
            });
            self.pane_resize = None;
            self.divider_drag = None;
            self.cancel_pending_key_chord();
            tui.frame_requester().schedule_frame();
            return Ok(true);
        }
        Ok(false)
    }

    async fn save_agents_overview_dock_width(
        &mut self,
        divider: DockDivider,
        width: u16,
        total: u16,
    ) {
        let percent = width_percent(width, total);
        let (saved, setting_name) = match divider {
            DockDivider::AgentsOverview => (
                self.local_settings.tui.agents_overview_width_percent,
                "agents_overview_width_percent",
            ),
            DockDivider::CommandReference => (
                self.local_settings.tui.command_reference_width_percent,
                "command_reference_width_percent",
            ),
        };
        if saved != Some(percent)
            && let Err(error) =
                ConfigEditsBuilder::for_config_path(self.local_settings.user_config_path.as_path())
                    .with_edits([ConfigEdit::SetPath {
                        segments: vec!["tui".into(), setting_name.into()],
                        value: i64::from(percent).into(),
                    }])
                    .apply()
                    .await
        {
            self.chat_widget
                .add_error_message(format!("Could not save dock width: {error}"));
            return;
        }
        match divider {
            DockDivider::AgentsOverview => {
                self.local_settings.tui.agents_overview_width_percent = Some(percent)
            }
            DockDivider::CommandReference => {
                self.local_settings.tui.command_reference_width_percent = Some(percent)
            }
        }
    }
}
