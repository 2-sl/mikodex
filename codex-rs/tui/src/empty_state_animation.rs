//! The blossom welcome animation for onboarding, with a full-color final pose.
//! Visible time pauses while hidden. Fresh conversations start settled and can replay on a click.

mod geometry;
mod lighting;
mod paths;
mod policy;
mod renderer;
mod sequence;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use std::time::Instant;

use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
use crossterm::event::MouseEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::motion::MotionMode;
use crate::terminal_palette;
use lighting::Lighting;
pub(crate) use policy::Presentation;
pub(crate) use policy::is_startup_cell;
use renderer::MAX_COLUMNS;
use renderer::MAX_ROWS;
use renderer::Renderer;

pub(crate) const FRAME_INTERVAL: Duration = Duration::from_millis(/*millis*/ 50);
// The preview's first visible pose (10.8s / 7.2s), also the start of an idle-screen replay.
const SETTLED_BLOSSOM: f64 = 0.5;
const BACKGROUND_IMAGE_DIRECTORY: &str = "background";
const BACKGROUND_IMAGE_FILENAME: &str = "custom.png";
const BACKGROUND_DISABLED_FILENAME: &str = "disabled";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposerState {
    Empty,
    Draft,
}

#[derive(Default)]
pub(crate) struct EmptyStateAnimation {
    eligible: bool,
    spin_elapsed: Duration,
    last_frame: Option<Instant>,
    fade_elapsed: Duration,
    static_mark: Option<bool>,
    opacity: f32,
    fade_from: f32,
    renderer: Option<Renderer>,
    stage: Option<Rect>,
    replaying: bool,
    background_image_path: Option<PathBuf>,
    background_disabled_path: Option<PathBuf>,
    background_image_cache: Option<(PathBuf, image::RgbaImage)>,
    chat_background_elapsed: Duration,
    chat_background_last_frame: Option<Instant>,
}

impl EmptyStateAnimation {
    pub(crate) fn for_home(home: &Path) -> Self {
        Self {
            background_image_path: Some(
                home.join(BACKGROUND_IMAGE_DIRECTORY)
                    .join(BACKGROUND_IMAGE_FILENAME),
            ),
            background_disabled_path: Some(
                home.join(BACKGROUND_IMAGE_DIRECTORY)
                    .join(BACKGROUND_DISABLED_FILENAME),
            ),
            ..Self::default()
        }
    }

    pub(crate) fn is_background_enabled(&self) -> bool {
        self.background_disabled_path
            .as_deref()
            .is_none_or(|path| !path.exists())
    }

    pub(crate) fn set_background_enabled(
        &mut self,
        enabled: bool,
        home: &Path,
    ) -> Result<(), String> {
        let path = home
            .join(BACKGROUND_IMAGE_DIRECTORY)
            .join(BACKGROUND_DISABLED_FILENAME);
        if enabled {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Could not enable the chat background ({}): {error}",
                        path.display()
                    ));
                }
            }
        } else {
            let directory = home.join(BACKGROUND_IMAGE_DIRECTORY);
            std::fs::create_dir_all(&directory).map_err(|error| {
                format!(
                    "Could not create chat background settings directory {}: {error}",
                    directory.display()
                )
            })?;
            std::fs::write(&path, b"disabled\n").map_err(|error| {
                format!(
                    "Could not disable the chat background ({}): {error}",
                    path.display()
                )
            })?;
        }
        self.background_disabled_path = Some(path);
        self.reset_chat_background();
        Ok(())
    }

    pub(crate) fn set_background_image(
        &mut self,
        source: &Path,
        home: &Path,
    ) -> Result<PathBuf, String> {
        let source = source.canonicalize().map_err(|error| {
            format!(
                "Could not open background image {}: {error}",
                source.display()
            )
        })?;
        let image = image::open(&source).map_err(|error| {
            format!(
                "Could not read background image {}: {error}",
                source.display()
            )
        })?;
        let directory = home.join(BACKGROUND_IMAGE_DIRECTORY);
        std::fs::create_dir_all(&directory).map_err(|error| {
            format!(
                "Could not create background image directory {}: {error}",
                directory.display()
            )
        })?;
        let destination = directory.join(BACKGROUND_IMAGE_FILENAME);
        let temporary = directory.join("custom.pending.png");
        image
            .save_with_format(&temporary, image::ImageFormat::Png)
            .map_err(|error| format!("Could not save background image: {error}"))?;
        if destination.exists() {
            std::fs::remove_file(&destination).map_err(|error| {
                format!(
                    "Could not replace background image {}: {error}",
                    destination.display()
                )
            })?;
        }
        std::fs::rename(&temporary, &destination).map_err(|error| {
            format!(
                "Could not install background image {}: {error}",
                destination.display()
            )
        })?;
        self.background_image_path = Some(destination.clone());
        self.background_image_cache = Some((destination.clone(), image.to_rgba8()));
        self.set_background_enabled(true, home)?;
        self.reset_chat_background();
        Ok(destination)
    }

    pub(crate) fn clear_background_image(&mut self, home: &Path) -> Result<(), String> {
        let path = home
            .join(BACKGROUND_IMAGE_DIRECTORY)
            .join(BACKGROUND_IMAGE_FILENAME);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "Could not remove background image {}: {error}",
                    path.display()
                ));
            }
        }
        self.background_image_path = Some(path);
        self.background_image_cache = None;
        self.reset_chat_background();
        Ok(())
    }

    pub(crate) fn background_image_path(&self) -> Option<&Path> {
        self.background_image_path
            .as_deref()
            .filter(|path| path.is_file())
    }

    fn reset_chat_background(&mut self) {
        self.chat_background_elapsed = Duration::ZERO;
        self.chat_background_last_frame = None;
        self.background_image_cache = None;
    }

    pub(crate) fn render_chat_background(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
    ) -> Option<Duration> {
        if !self.is_background_enabled()
            || area.is_empty()
            || area.width > MAX_COLUMNS.saturating_mul(4)
            || area.height > MAX_ROWS.saturating_mul(4)
            || area.intersection(buffer.area) != area
        {
            self.chat_background_last_frame = None;
            return None;
        }
        let now = Instant::now();
        let elapsed = self
            .chat_background_last_frame
            .replace(now)
            .map(|previous| now.saturating_duration_since(previous))
            .unwrap_or_default();
        self.chat_background_elapsed += elapsed;
        let image_path = self.background_image_path().map(Path::to_path_buf);
        if let Some(path) = image_path {
            let cache_matches = self
                .background_image_cache
                .as_ref()
                .is_some_and(|(cached_path, _)| cached_path == &path);
            if !cache_matches {
                self.background_image_cache = image::open(&path)
                    .ok()
                    .map(|image| (path.clone(), image.to_rgba8()));
            }
        } else {
            self.background_image_cache = None;
        }

        if let Some((_, image)) = self.background_image_cache.as_ref() {
            self.paint_chat_background_image(area, buffer, image);
        } else {
            self.paint_chat_background_logo(area, buffer);
        }
        Some(FRAME_INTERVAL)
    }

    fn paint_chat_background_logo(&mut self, area: Rect, buffer: &mut Buffer) {
        let width = area.width.min(MAX_COLUMNS);
        let height = ((u32::from(width) * 21 / 60) as u16)
            .min(area.height)
            .min(MAX_ROWS);
        if width == 0 || height == 0 {
            return;
        }
        let stage = Rect::new(
            area.x + area.width.saturating_sub(width) / 2,
            area.y + area.height.saturating_sub(height) / 2,
            width,
            height,
        );
        let background = terminal_palette::default_bg().unwrap_or((15, 20, 37));
        let color_level = terminal_palette::effective_stdout_color_level();
        let lighting = Lighting::terminal(
            terminal_palette::default_fg().unwrap_or((210, 221, 235)),
            background,
        );
        let phase = self.chat_background_elapsed.as_secs_f64() / sequence::LOOP_SECONDS;
        let cells = self
            .renderer
            .get_or_insert_with(Renderer::default)
            .frame(width, height, phase, &lighting);
        for (index, cell) in cells.iter().enumerate() {
            if cell.dots == 0 {
                continue;
            }
            let point = (
                stage.x + (index % usize::from(width)) as u16,
                stage.y + (index / usize::from(width)) as u16,
            );
            let target = &mut buffer[point];
            if target.symbol() != " " {
                continue;
            }
            let [_, red, green, blue] = cell.rgb.to_be_bytes();
            target
                .set_char(char::from_u32(0x2800 + u32::from(cell.dots)).unwrap_or(' '))
                .set_fg(background_terminal_color(
                    (red, green, blue),
                    background,
                    /*opacity*/ 0.38,
                    color_level,
                ));
        }
    }

    fn paint_chat_background_image(
        &self,
        area: Rect,
        buffer: &mut Buffer,
        image: &image::RgbaImage,
    ) {
        if image.width() == 0 || image.height() == 0 {
            return;
        }
        let target_width = area.width.min(MAX_COLUMNS).max(1);
        let target_height = area.height.min(MAX_ROWS).max(1);
        let pixel_width = target_width.saturating_mul(2);
        let pixel_height = target_height.saturating_mul(2);
        let fitted = image::imageops::resize(
            image,
            u32::from(pixel_width),
            u32::from(pixel_height),
            image::imageops::FilterType::Triangle,
        );
        let stage = Rect::new(
            area.x + area.width.saturating_sub(target_width) / 2,
            area.y + area.height.saturating_sub(target_height) / 2,
            target_width,
            target_height,
        );
        let bob = (self.chat_background_elapsed.as_secs_f64() * std::f64::consts::TAU / 8.0).sin();
        let opacity = (0.28 + bob * 0.035) as f32;
        let background = terminal_palette::default_bg().unwrap_or((15, 20, 37));
        let color_level = terminal_palette::effective_stdout_color_level();
        let vertical_shift = if bob > 0.65 {
            -1_i32
        } else if bob < -0.65 {
            1
        } else {
            0
        };
        for y in 0..target_height {
            for x in 0..target_width {
                let target_y = i32::from(stage.y + y) + vertical_shift;
                if target_y < i32::from(stage.y) || target_y >= i32::from(stage.bottom()) {
                    continue;
                }
                let cell = &mut buffer[(stage.x + x, target_y as u16)];
                if cell.symbol() != " " {
                    continue;
                }
                let top = fitted.get_pixel(u32::from(x) * 2, u32::from(y) * 2).0;
                let bottom = fitted.get_pixel(u32::from(x) * 2, u32::from(y) * 2 + 1).0;
                if top[3] == 0 && bottom[3] == 0 {
                    continue;
                }
                let mix = |pixel: [u8; 4]| {
                    background_terminal_color(
                        (pixel[0], pixel[1], pixel[2]),
                        background,
                        opacity * f32::from(pixel[3]) / 255.0,
                        color_level,
                    )
                };
                if top[3] > 0 {
                    cell.set_char('?').set_fg(mix(top));
                }
                if bottom[3] > 0 {
                    cell.set_bg(mix(bottom));
                }
            }
        }
    }

    pub(crate) fn is_eligible(&self) -> bool {
        self.eligible
    }

    pub(crate) fn start_fresh(&mut self) {
        self.cancel_replay();
        self.eligible = true;
        self.spin_elapsed = Duration::ZERO;
        self.last_frame = None;
        self.fade_elapsed = Duration::ZERO;
        self.static_mark = None;
        self.opacity = 1.0;
    }

    /// Keep the provisional pose when handing off to the live thread.
    pub(crate) fn continue_from(&mut self, source: &mut Self) {
        let mut previous = std::mem::take(source);
        if !previous.is_eligible() {
            previous.start_fresh();
        }
        *self = previous;
    }

    pub(crate) fn dismiss(&mut self) {
        self.eligible = false;
        self.cancel_replay();
    }

    pub(crate) fn cancel_replay(&mut self) {
        self.stage = None;
        self.replaying = false;
        self.pause_clock();
    }

    /// Replay only on the drawn blossom; unrelated gestures keep their normal owner.
    pub(crate) fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        let inside = self
            .stage
            .is_some_and(|stage| stage.contains(Position::new(mouse.column, mouse.row)))
            && mouse.modifiers.is_empty();
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) if inside => {
                self.start_fresh();
                self.spin_elapsed =
                    Duration::from_secs_f64(SETTLED_BLOSSOM * sequence::LOOP_SECONDS);
                self.opacity = sequence::STATIC_OPACITY;
                self.static_mark = Some(true);
                self.replaying = true;
                true
            }
            _ => false,
        }
    }

    /// Stop visible time at the last painted frame, including when no hidden frame was drawn.
    /// Resume (after process suspension) may call this after the interruption; phase is retained.
    pub(crate) fn pause_clock(&mut self) {
        self.last_frame = None;
    }

    /// Paint the shared logo sequence inside a caller-owned, reserved rectangle.
    /// The caller clears the stage and keeps its layout stable after motion finishes.
    pub(crate) fn render_in(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
        presentation: Presentation,
    ) -> Option<Duration> {
        let now = Instant::now();
        self.render_in_at(area, buffer, presentation, now)
    }

    fn render_in_at(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
        presentation: Presentation,
        now: Instant,
    ) -> Option<Duration> {
        if !self.eligible
            || presentation == Presentation::Hidden
            || area.is_empty()
            || area.width > MAX_COLUMNS
            || area.height > MAX_ROWS
            || area.intersection(buffer.area) != area
        {
            self.pause_clock();
            return None;
        }
        let static_mark = presentation == Presentation::Faded;
        let previous_frame = self.last_frame.replace(now);
        if self.static_mark != Some(static_mark) {
            self.fade_elapsed = if self.static_mark.is_none() {
                sequence::STATIC_FADE
            } else {
                Duration::ZERO
            };
            self.fade_from = self.opacity;
        } else if let Some(previous) = previous_frame {
            let elapsed = now.saturating_duration_since(previous);
            self.fade_elapsed += elapsed;
            if !static_mark {
                self.spin_elapsed += elapsed;
            }
        }
        self.static_mark = Some(static_mark);
        let finished = self.spin_elapsed >= sequence::SPIN_DURATION;
        let phase =
            self.spin_elapsed.min(sequence::SPIN_DURATION).as_secs_f64() / sequence::LOOP_SECONDS;
        let settling = !finished && static_mark && self.fade_elapsed < sequence::STATIC_FADE;
        self.opacity = if finished && self.replaying {
            sequence::static_opacity(
                self.spin_elapsed - sequence::SPIN_DURATION,
                /*from*/ 1.0,
            )
        } else if finished {
            1.0
        } else if static_mark {
            sequence::static_opacity(self.fade_elapsed, self.fade_from)
        } else {
            1.0
        };
        if !static_mark && !finished {
            self.opacity = self.fade_from
                + (self.opacity - self.fade_from)
                    * sequence::progress(self.fade_elapsed, sequence::STATIC_FADE) as f32;
        }
        self.paint_frame(area, buffer, phase, self.opacity);
        ((!finished && (!static_mark || settling))
            || (self.replaying
                && self.spin_elapsed < sequence::SPIN_DURATION + sequence::STATIC_FADE))
            .then_some(FRAME_INTERVAL)
    }

    /// Draw only in space the caller has cleared and owns. The centered blossom returns
    /// on clear, without restarting or scheduling animation.
    pub(crate) fn render_first_screen(
        &mut self,
        screen: Rect,
        available: Rect,
        buffer: &mut Buffer,
        composer: Option<ComposerState>,
        motion: MotionMode,
    ) -> Option<Duration> {
        const STAGE_ROWS: u16 = 21;
        const MIN_STAGE_ROWS: u16 = 14;
        let width = screen.width.saturating_sub(/*rhs*/ 4).min(MAX_COLUMNS);
        let height = width * STAGE_ROWS / MAX_COLUMNS;
        if motion == MotionMode::Reduced
            || composer != Some(ComposerState::Empty)
            || height < MIN_STAGE_ROWS
            || height > screen.height
        {
            self.cancel_replay();
            return None;
        }
        let stage = Rect::new(
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
            width,
            height,
        );
        if stage.intersection(available) != stage {
            self.cancel_replay();
            return None;
        }
        self.stage = Some(stage);
        if self.replaying {
            if let Some(delay) = self.render_in(stage, buffer, Presentation::Animated) {
                return Some(delay);
            }
            // The replay has faded back to the idle pose and no longer needs redraws.
            self.replaying = false;
        }
        self.paint_frame(stage, buffer, SETTLED_BLOSSOM, sequence::STATIC_OPACITY);
        None
    }

    fn paint_frame(&mut self, area: Rect, buffer: &mut Buffer, phase: f64, opacity: f32) {
        let background = terminal_palette::default_bg();
        let color_level = if background.is_some() {
            terminal_palette::effective_stdout_color_level()
        } else {
            terminal_palette::StdoutColorLevel::Unknown
        };
        let background = background.unwrap_or((15, 20, 37));
        let light = Lighting::terminal(
            terminal_palette::default_fg().unwrap_or((210, 221, 235)),
            background,
        );
        let cells = self.renderer.get_or_insert_with(Renderer::default).frame(
            area.width,
            area.height,
            phase,
            &light,
        );
        for (i, cell) in cells.iter().enumerate().filter(|(_, cell)| cell.dots != 0) {
            let [_, r, g, b] = cell.rgb.to_be_bytes();
            let target = &mut buffer[(
                area.x + i as u16 % area.width,
                area.y + i as u16 / area.width,
            )];
            target.set_char(char::from_u32(0x2800 + u32::from(cell.dots)).unwrap_or(' '));
            let color = crate::color::blend((r, g, b), background, opacity);
            let color = if color_level == terminal_palette::StdoutColorLevel::Ansi256 {
                // Four bits per channel bound the cache and avoid palette searches each frame.
                static COLORS: [OnceLock<Color>; 4096] = [const { OnceLock::new() }; 4096];
                let (r, g, b) = (color.0 >> 4, color.1 >> 4, color.2 >> 4);
                let index = usize::from(r) * 256 + usize::from(g) * 16 + usize::from(b);
                *COLORS[index].get_or_init(|| {
                    terminal_palette::best_color_for_level(
                        (r * 16 + 8, g * 16 + 8, b * 16 + 8),
                        color_level,
                    )
                })
            } else {
                terminal_palette::best_color_for_level(color, color_level)
            };
            target.set_fg(color);
            // Default-color terminals can only step between normal and dim intensity.
            if opacity < 0.5
                && matches!(
                    color_level,
                    terminal_palette::StdoutColorLevel::Ansi16
                        | terminal_palette::StdoutColorLevel::Unknown
                )
            {
                target.set_style(target.style().dim());
            }
        }
    }
}

fn background_terminal_color(
    rgb: (u8, u8, u8),
    background: (u8, u8, u8),
    opacity: f32,
    color_level: terminal_palette::StdoutColorLevel,
) -> Color {
    let color = crate::color::blend(rgb, background, opacity);
    if color_level == terminal_palette::StdoutColorLevel::Ansi256 {
        static COLORS: [OnceLock<Color>; 4096] = [const { OnceLock::new() }; 4096];
        let (r, g, b) = (color.0 >> 4, color.1 >> 4, color.2 >> 4);
        let index = usize::from(r) * 256 + usize::from(g) * 16 + usize::from(b);
        *COLORS[index].get_or_init(|| {
            terminal_palette::best_color_for_level(
                (r * 16 + 8, g * 16 + 8, b * 16 + 8),
                color_level,
            )
        })
    } else {
        terminal_palette::best_color_for_level(color, color_level)
    }
}

#[cfg(test)]
#[path = "empty_state_animation_tests.rs"]
mod tests;
