//! The user-selected image animation. Visible time pauses while hidden.

// Keep the original logo geometry and renderer source available, but don't draw it in the TUI.
#[allow(dead_code)]
mod geometry;
#[allow(dead_code)]
mod lighting;
#[allow(dead_code)]
mod paths;
mod policy;
#[allow(dead_code)]
mod renderer;
mod sequence;

use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

#[cfg(test)]
use crossterm::event::MouseButton;
use crossterm::event::MouseEvent;
#[cfg(test)]
use crossterm::event::MouseEventKind;
use image::RgbaImage;
use image::imageops::FilterType;
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::widgets::Widget;

use crate::motion::MotionMode;
use crate::terminal_palette;
#[cfg(test)]
use lighting::Lighting;
pub(crate) use policy::Presentation;
pub(crate) use policy::is_startup_cell;
use renderer::MAX_COLUMNS;
use renderer::MAX_ROWS;
#[cfg(test)]
use renderer::Renderer;

pub(crate) const FRAME_INTERVAL: Duration = Duration::from_millis(/*millis*/ 50);
const MAX_BOB_SUBPIXEL_ROWS: i16 = 6;
const BACKGROUND_IMAGE_DIRECTORY: &str = "background";
const BACKGROUND_IMAGE_FILENAME: &str = "custom.png";
static BACKGROUND_IMAGE_REVISION: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy)]
struct SpriteCell {
    dots: u8,
    rgb: (u8, u8, u8),
    background_rgb: Option<(u8, u8, u8)>,
}

struct SpriteFrame {
    area_width: u16,
    area_height: u16,
    image_revision: u64,
    portrait_size: (u16, u16),
    portrait_frames: Vec<Vec<Option<SpriteCell>>>,
}
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
    image_elapsed: Duration,
    chat_background_elapsed: Duration,
    chat_background_last_frame: Option<Instant>,
    background_image_path: Option<PathBuf>,
    sprite_frame: Option<SpriteFrame>,
    stage: Option<Rect>,
    replaying: bool,
}

impl EmptyStateAnimation {
    pub(crate) fn for_home(home: &Path) -> Self {
        Self {
            background_image_path: Some(background_image_path(home)),
            ..Self::default()
        }
    }

    pub(crate) fn for_current_home() -> Self {
        codex_utils_home_dir::find_codex_home()
            .map(|home| Self::for_home(home.as_path()))
            .unwrap_or_default()
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
        self.image_elapsed = Duration::ZERO;
        self.chat_background_elapsed = Duration::ZERO;
        self.chat_background_last_frame = None;
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
        let destination = background_image_path(home);
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
        self.reset_background_image_frame();
        Ok(destination)
    }

    pub(crate) fn clear_background_image(&mut self, home: &Path) -> Result<(), String> {
        let path = background_image_path(home);
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
        self.reset_background_image_frame();
        Ok(())
    }

    pub(crate) fn background_image_path(&self) -> Option<&Path> {
        self.background_image_path
            .as_deref()
            .filter(|path| path.is_file())
    }

    fn reset_background_image_frame(&mut self) {
        BACKGROUND_IMAGE_REVISION.fetch_add(1, Ordering::Relaxed);
        self.sprite_frame = None;
        self.image_elapsed = Duration::ZERO;
        self.chat_background_elapsed = Duration::ZERO;
        self.chat_background_last_frame = None;
    }

    pub(crate) fn cancel_replay(&mut self) {
        self.stage = None;
        self.replaying = false;
        self.pause_clock();
    }

    /// Leave mouse input to the composer; the sprite animates continuously on its own.
    pub(crate) fn handle_mouse(&mut self, _mouse: MouseEvent) -> bool {
        false
    }

    /// Stop visible time at the last painted frame, including when no hidden frame was drawn.
    /// Resume after process suspension continues from the last visible bob position.
    pub(crate) fn pause_clock(&mut self) {
        self.last_frame = None;
    }

    /// Paint the selected image inside a caller-owned, reserved rectangle.
    /// The caller clears the stage and schedules another frame while it stays visible.
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
            || !self.has_background_image()
        {
            self.pause_clock();
            return None;
        }
        let static_mark = presentation == Presentation::Faded;
        let previous_frame = self.last_frame.replace(now);
        if let Some(previous) = previous_frame {
            self.image_elapsed += now.saturating_duration_since(previous);
        }
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
        self.paint_frame(area, buffer, self.image_elapsed.as_secs_f64(), self.opacity);
        Some(FRAME_INTERVAL)
    }

    /// Draw only in space the caller has cleared and owns, while the sprite keeps floating.
    pub(crate) fn render_first_screen(
        &mut self,
        screen: Rect,
        available: Rect,
        buffer: &mut Buffer,
        composer: Option<ComposerState>,
        _motion: MotionMode,
    ) -> Option<Duration> {
        const STAGE_ROWS: u16 = 63;
        const MIN_STAGE_ROWS: u16 = 14;
        if !self.has_background_image() {
            self.cancel_replay();
            return None;
        }
        let height_limited_width = (u32::from(screen.height) * u32::from(MAX_COLUMNS)
            / u32::from(STAGE_ROWS))
        .min(u32::from(MAX_COLUMNS)) as u16;
        let width = screen
            .width
            .saturating_sub(/*rhs*/ 4)
            .min(MAX_COLUMNS)
            .min(height_limited_width);
        let height = width * STAGE_ROWS / MAX_COLUMNS;
        if composer != Some(ComposerState::Empty)
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
        if stage.intersection(buffer.area) == stage {
            // Clear the reserved animation stage before painting so moving frames leave no trail.
            ratatui::widgets::Clear.render(stage, buffer);
        }
        self.render_in(stage, buffer, Presentation::Animated)
    }

    /// Paint the selected image as a faint watermark in unused transcript cells.
    pub(crate) fn render_chat_background(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
    ) -> Option<Duration> {
        const STAGE_ROWS: u16 = 63;
        const MIN_STAGE_ROWS: u16 = 14;
        if !self.has_background_image() {
            self.chat_background_last_frame = None;
            return None;
        }
        let height_limited_width = (u32::from(area.height) * u32::from(MAX_COLUMNS)
            / u32::from(STAGE_ROWS))
        .min(u32::from(MAX_COLUMNS)) as u16;
        let width = area
            .width
            .saturating_sub(/*rhs*/ 4)
            .min(MAX_COLUMNS)
            .min(height_limited_width);
        let height = width * STAGE_ROWS / MAX_COLUMNS;
        if width == 0 || height < MIN_STAGE_ROWS || height > area.height {
            self.chat_background_last_frame = None;
            return None;
        }
        let stage = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        if stage.intersection(buffer.area) != stage {
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
        self.paint_background_frame(stage, buffer, self.chat_background_elapsed.as_secs_f64());
        Some(FRAME_INTERVAL)
    }

    fn paint_frame(&mut self, area: Rect, buffer: &mut Buffer, bob_seconds: f64, opacity: f32) {
        self.paint_frame_with_options(
            area,
            buffer,
            bob_seconds,
            opacity.max(0.72),
            /*only_blank_cells*/ false,
            /*paint_cell_backgrounds*/ true,
        );
    }

    fn paint_background_frame(&mut self, area: Rect, buffer: &mut Buffer, bob_seconds: f64) {
        self.paint_frame_with_options(
            area,
            buffer,
            bob_seconds,
            /*opacity*/ 0.5,
            /*only_blank_cells*/ true,
            /*paint_cell_backgrounds*/ false,
        );
    }

    fn paint_frame_with_options(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
        bob_seconds: f64,
        opacity: f32,
        only_blank_cells: bool,
        paint_cell_backgrounds: bool,
    ) {
        let background = terminal_palette::default_bg();
        let color_level = if background.is_some() {
            terminal_palette::effective_stdout_color_level()
        } else {
            terminal_palette::StdoutColorLevel::Unknown
        };
        let background = background.unwrap_or((15, 20, 37));
        self.paint_image(
            area,
            buffer,
            bob_seconds,
            opacity,
            only_blank_cells,
            paint_cell_backgrounds,
            color_level,
            background,
        );
    }

    fn paint_image(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
        bob_seconds: f64,
        opacity: f32,
        only_blank_cells: bool,
        paint_cell_backgrounds: bool,
        color_level: terminal_palette::StdoutColorLevel,
        background: (u8, u8, u8),
    ) {
        if !self.ensure_sprite_frame(area.width, area.height) {
            return;
        }
        let Some(frame) = self.sprite_frame.as_ref() else {
            return;
        };
        const BOB_PERIOD_SECONDS: f64 = 4.8;
        const BOB_AMPLITUDE_ROWS: f64 = 1.5;
        let bob_phase = bob_seconds * std::f64::consts::TAU / BOB_PERIOD_SECONDS;
        const SUBPIXELS_PER_ROW: f64 = 4.0;
        let bob_offset = (bob_phase.sin() * BOB_AMPLITUDE_ROWS * SUBPIXELS_PER_ROW).round() as i16;
        let frame_index = usize::try_from(bob_offset + MAX_BOB_SUBPIXEL_ROWS)
            .unwrap_or_default()
            .min(frame.portrait_frames.len().saturating_sub(1));
        let portrait = &frame.portrait_frames[frame_index];
        let origin_x = i32::from(area.x + area.width.saturating_sub(frame.portrait_size.0) / 2);
        let origin_y = i32::from(area.y + area.height.saturating_sub(frame.portrait_size.1) / 2);
        paint_sprite(
            area,
            buffer,
            origin_x,
            origin_y,
            frame.portrait_size,
            portrait,
            opacity,
            only_blank_cells,
            paint_cell_backgrounds,
            color_level,
            background,
        );
    }

    fn ensure_sprite_frame(&mut self, area_width: u16, area_height: u16) -> bool {
        let image_revision = BACKGROUND_IMAGE_REVISION.load(Ordering::Relaxed);
        let matches = self.sprite_frame.as_ref().is_some_and(|frame| {
            frame.area_width == area_width
                && frame.area_height == area_height
                && frame.image_revision == image_revision
        });
        if !matches {
            self.sprite_frame = self
                .background_image_path
                .as_deref()
                .and_then(|path| build_image_frame(path, area_width, area_height, image_revision));
        }
        self.sprite_frame.is_some()
    }

    fn has_background_image(&self) -> bool {
        self.background_image_path
            .as_deref()
            .is_some_and(Path::is_file)
    }
}

fn background_image_path(home: &Path) -> PathBuf {
    home.join(BACKGROUND_IMAGE_DIRECTORY)
        .join(BACKGROUND_IMAGE_FILENAME)
}

fn build_image_frame(
    image_path: &Path,
    area_width: u16,
    area_height: u16,
    image_revision: u64,
) -> Option<SpriteFrame> {
    let portrait = image::open(image_path).ok()?.to_rgba8();
    let max_portrait_height = area_height.saturating_sub(4).max(1);
    let portrait_size = fit_sprite(
        portrait.width(),
        portrait.height(),
        area_width.saturating_sub(4),
        max_portrait_height,
    );
    let resized_portrait = image::imageops::resize(
        &portrait,
        u32::from(portrait_size.0) * 2,
        u32::from(portrait_size.1) * 4,
        FilterType::Lanczos3,
    );
    const MAX_BOB_SUBPIXEL_ROWS: i16 = 6;
    let portrait_frames = (-MAX_BOB_SUBPIXEL_ROWS..=MAX_BOB_SUBPIXEL_ROWS)
        .map(|offset| rasterize_sprite(&resized_portrait, portrait_size.0, portrait_size.1, offset))
        .collect();
    Some(SpriteFrame {
        area_width,
        area_height,
        image_revision,
        portrait_frames,
        portrait_size,
    })
}

fn fit_sprite(
    source_width: u32,
    source_height: u32,
    max_width: u16,
    max_height: u16,
) -> (u16, u16) {
    if source_width == 0 || source_height == 0 || max_width == 0 || max_height == 0 {
        return (1, 1);
    }
    // A Braille cell holds 2x4 pixels, so its terminal-cell aspect ratio is twice the image ratio.
    let columns_per_row = 2.0 * source_width as f32 / source_height as f32;
    let mut height = max_height;
    let mut width = (columns_per_row * f32::from(height)).round().max(1.0) as u16;
    if width > max_width {
        width = max_width;
        height = (f32::from(width) / columns_per_row).round().max(1.0) as u16;
    }
    (width.min(max_width).max(1), height.min(max_height).max(1))
}

fn rasterize_sprite(
    pixels: &RgbaImage,
    width: u16,
    height: u16,
    vertical_offset: i16,
) -> Vec<Option<SpriteCell>> {
    const DOT_BITS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];
    const ALPHA_THRESHOLD: u8 = 112;
    const COLOR_DETAIL_THRESHOLD: u8 = 24;
    let mut cells = vec![None; usize::from(width) * usize::from(height)];
    for cell_y in 0..height {
        for cell_x in 0..width {
            let mut samples = [[0u8; 4]; 8];
            for dot_y in 0..4 {
                for dot_x in 0..2 {
                    let sample_index = dot_y as usize * 2 + dot_x as usize;
                    let pixel_y =
                        (u32::from(cell_y) * 4 + dot_y) as i32 - i32::from(vertical_offset);
                    samples[sample_index] = if pixel_y >= 0 && pixel_y < pixels.height() as i32 {
                        pixels
                            .get_pixel(u32::from(cell_x) * 2 + dot_x, pixel_y as u32)
                            .0
                    } else {
                        [0; 4]
                    };
                }
            }

            let fully_opaque = samples.iter().all(|sample| sample[3] >= 248);
            if fully_opaque {
                if let Some(cell) = two_tone_cell(&samples, COLOR_DETAIL_THRESHOLD, &DOT_BITS) {
                    cells[usize::from(cell_y) * usize::from(width) + usize::from(cell_x)] =
                        Some(cell);
                    continue;
                }
            }

            let mut dots = 0;
            let mut red = 0u64;
            let mut green = 0u64;
            let mut blue = 0u64;
            let mut weight = 0u64;
            for (sample_index, sample) in samples.iter().enumerate() {
                if sample[3] < ALPHA_THRESHOLD {
                    continue;
                }
                dots |= DOT_BITS[sample_index / 2][sample_index % 2];
                let alpha = u64::from(sample[3]);
                red += u64::from(sample[0]) * alpha;
                green += u64::from(sample[1]) * alpha;
                blue += u64::from(sample[2]) * alpha;
                weight += alpha;
            }
            if weight > 0 {
                cells[usize::from(cell_y) * usize::from(width) + usize::from(cell_x)] =
                    Some(SpriteCell {
                        dots,
                        rgb: (
                            (red / weight) as u8,
                            (green / weight) as u8,
                            (blue / weight) as u8,
                        ),
                        background_rgb: None,
                    });
            }
        }
    }
    cells
}

fn two_tone_cell(
    samples: &[[u8; 4]; 8],
    detail_threshold: u8,
    dot_bits: &[[u8; 2]; 4],
) -> Option<SpriteCell> {
    let luminance = samples.map(|sample| {
        (u16::from(sample[0]) * 54 + u16::from(sample[1]) * 183 + u16::from(sample[2]) * 19) / 256
    });
    let mut low = *luminance.iter().min()?;
    let mut high = *luminance.iter().max()?;
    if high.saturating_sub(low) < u16::from(detail_threshold) {
        return None;
    }

    let mut high_group = [false; 8];
    for _ in 0..4 {
        let mut low_sum = 0u32;
        let mut low_count = 0u32;
        let mut high_sum = 0u32;
        let mut high_count = 0u32;
        for (index, value) in luminance.iter().enumerate() {
            let use_high = high.abs_diff(*value) < low.abs_diff(*value);
            high_group[index] = use_high;
            if use_high {
                high_sum += u32::from(*value);
                high_count += 1;
            } else {
                low_sum += u32::from(*value);
                low_count += 1;
            }
        }
        if low_count == 0 || high_count == 0 {
            return None;
        }
        low = (low_sum / low_count) as u16;
        high = (high_sum / high_count) as u16;
    }

    let high_group_count = high_group.iter().filter(|&&is_high| is_high).count() as u32;
    let foreground_is_high = high_group_count <= 4;
    let mut foreground_color = [0u32; 3];
    let mut foreground_count = 0u32;
    let mut background_color = [0u32; 3];
    let mut background_count = 0u32;
    let mut dots = 0;
    for (index, sample) in samples.iter().enumerate() {
        let is_foreground = high_group[index] == foreground_is_high;
        let target = if is_foreground {
            dots |= dot_bits[index / 2][index % 2];
            foreground_count += 1;
            &mut foreground_color
        } else {
            background_count += 1;
            &mut background_color
        };
        target[0] += u32::from(sample[0]);
        target[1] += u32::from(sample[1]);
        target[2] += u32::from(sample[2]);
    }
    if background_count == 0 || foreground_count == 0 {
        return None;
    }
    Some(SpriteCell {
        dots,
        rgb: (
            (foreground_color[0] / foreground_count) as u8,
            (foreground_color[1] / foreground_count) as u8,
            (foreground_color[2] / foreground_count) as u8,
        ),
        background_rgb: Some((
            (background_color[0] / background_count) as u8,
            (background_color[1] / background_count) as u8,
            (background_color[2] / background_count) as u8,
        )),
    })
}

#[allow(clippy::too_many_arguments)]
fn paint_sprite(
    area: Rect,
    buffer: &mut Buffer,
    origin_x: i32,
    origin_y: i32,
    size: (u16, u16),
    cells: &[Option<SpriteCell>],
    opacity: f32,
    only_blank_cells: bool,
    paint_cell_backgrounds: bool,
    color_level: terminal_palette::StdoutColorLevel,
    background: (u8, u8, u8),
) {
    for y in 0..size.1 {
        for x in 0..size.0 {
            let Some(cell) = cells[usize::from(y) * usize::from(size.0) + usize::from(x)] else {
                continue;
            };
            let target_x = origin_x + i32::from(x);
            let target_y = origin_y + i32::from(y);
            if target_x < 0
                || target_y < 0
                || target_x > i32::from(u16::MAX)
                || target_y > i32::from(u16::MAX)
            {
                continue;
            }
            let (target_x, target_y) = (target_x as u16, target_y as u16);
            if !area.contains(Position::new(target_x, target_y)) {
                continue;
            }
            let target = &mut buffer[(target_x, target_y)];
            if only_blank_cells && target.symbol() != " " {
                continue;
            }
            if only_blank_cells {
                target.set_style(target.style().remove_modifier(Modifier::DIM));
            }
            let color = terminal_color(cell.rgb, background, opacity, color_level);
            target.set_char(char::from_u32(0x2800 + u32::from(cell.dots)).unwrap_or(' '));
            target.set_fg(color);
            if paint_cell_backgrounds && let Some(rgb) = cell.background_rgb {
                target.set_bg(terminal_color(rgb, background, opacity, color_level));
            }
        }
    }
}

fn terminal_color(
    rgb: (u8, u8, u8),
    background: (u8, u8, u8),
    opacity: f32,
    color_level: terminal_palette::StdoutColorLevel,
) -> Color {
    let color = crate::color::blend(rgb, background, opacity);
    if color_level == terminal_palette::StdoutColorLevel::Ansi256 {
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
    }
}

#[cfg(test)]
#[path = "empty_state_animation_tests.rs"]
mod tests;
