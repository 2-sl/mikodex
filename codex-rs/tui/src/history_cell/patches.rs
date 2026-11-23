//! Patch summaries and image-tool transcript helpers.

use super::*;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::terminal_hyperlinks::TerminalHyperlink;
use crate::terminal_hyperlinks::adaptive_wrap_hyperlink_lines;
use codex_ansi_escape::ansi_escape;
use codex_utils_path_uri::LegacyAppPathString;

#[cfg(test)]
#[path = "patches_tests.rs"]
mod tests;

#[derive(Debug)]
pub(crate) struct PatchHistoryCell {
    activity_id: String,
    changes: HashMap<PathBuf, FileChange>,
    cwd: PathBuf,
    timestamp: Option<String>,
}

impl PatchHistoryCell {
    pub(crate) fn with_activity_id(mut self, id: String) -> Self {
        self.activity_id = format!("patch:{id}");
        self
    }

    pub(crate) fn with_timestamp(
        mut self,
        timestamp: Option<chrono::DateTime<chrono::Local>>,
    ) -> Self {
        if let Some(timestamp) = timestamp {
            self.timestamp = Some(crate::clock_format::format_local_timestamp(timestamp));
        }
        self
    }

    fn timestamp_line(&self) -> Option<Line<'static>> {
        self.timestamp
            .as_ref()
            .map(|timestamp| Line::from(vec!["  \u{2022} ".dim(), timestamp.clone().dim()]))
    }
}

impl HistoryCell for PatchHistoryCell {
    fn tool_call_labels(&self) -> Vec<String> {
        let mut paths = self.changes.keys();
        let label = match (paths.next(), paths.next()) {
            (Some(path), None) => format!(
                "Edited {}",
                path.file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
            ),
            _ => "Edited files".into(),
        };
        vec![label]
    }

    fn activity_ids(&self) -> Vec<String> {
        vec![self.activity_id.clone()]
    }

    fn compact_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = crate::diff_render::create_diff_preview_with_links(
            &self.changes,
            &self.cwd,
            usize::from(width),
            super::activity_preview::DETAIL_PREVIEW_LINES,
        );
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp.into());
        }
        lines
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        visible_lines(self.display_hyperlink_lines(width))
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = crate::diff_render::create_diff_summary_with_links(
            &self.changes,
            &self.cwd,
            usize::from(width),
        );
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp.into());
        }
        lines
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        self.display_hyperlink_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = create_diff_summary(&self.changes, &self.cwd, RAW_DIFF_SUMMARY_WIDTH);
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp);
        }
        plain_lines(lines)
    }
}
/// Create a new `PendingPatch` cell that lists the file‑level summary of
/// a proposed patch. The summary lines should already be formatted (e.g.
/// "A path/to/file.rs").
pub(crate) fn new_patch_event(
    changes: HashMap<PathBuf, FileChange>,
    cwd: &Path,
) -> PatchHistoryCell {
    PatchHistoryCell {
        activity_id: format!("patch:{}", uuid::Uuid::new_v4()),
        changes,
        cwd: cwd.to_path_buf(),
        timestamp: Some(crate::clock_format::format_local_timestamp(
            chrono::Local::now(),
        )),
    }
}

pub(crate) fn new_patch_apply_failure(stderr: String) -> PatchFailureCell {
    PatchFailureCell {
        activity_id: format!("patch-failure:{}", uuid::Uuid::new_v4()),
        stderr,
        timestamp: Some(crate::clock_format::format_local_timestamp(
            chrono::Local::now(),
        )),
    }
}

/// Failed patch attempts retain available diagnostics for local disclosure and full transcript.
#[derive(Debug)]
pub(crate) struct PatchFailureCell {
    activity_id: String,
    stderr: String,
    timestamp: Option<String>,
}

impl PatchFailureCell {
    pub(crate) fn with_timestamp(
        mut self,
        timestamp: Option<chrono::DateTime<chrono::Local>>,
    ) -> Self {
        if let Some(timestamp) = timestamp {
            self.timestamp = Some(crate::clock_format::format_local_timestamp(timestamp));
        }
        self
    }

    fn timestamp_line(&self) -> Option<Line<'static>> {
        self.timestamp
            .as_ref()
            .map(|timestamp| Line::from(vec!["  \u{2022} ".dim(), timestamp.clone().dim()]))
    }
}

impl HistoryCell for PatchFailureCell {
    fn tool_call_labels(&self) -> Vec<String> {
        vec!["Failed to apply patch".into()]
    }

    fn activity_ids(&self) -> Vec<String> {
        vec![self.activity_id.clone()]
    }

    fn compact_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = self.transcript_hyperlink_lines(width);
        lines.truncate(1 + super::activity_preview::DETAIL_PREVIEW_LINES);
        lines
            .into_iter()
            .map(|line| super::activity_preview::clipped_line(line.line, width))
            .collect()
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = vec![Line::from("✘ Failed to apply patch".magenta().bold()).into()];
        let error = if self.stderr.trim().is_empty() {
            "(error details unavailable)"
        } else {
            &self.stderr
        };
        let mut diagnostics = ansi_escape(&error.replace('\t', "    ")).lines;
        for line in &mut diagnostics {
            for span in &mut line.spans {
                span.style = span.style.add_modifier(Modifier::DIM);
            }
        }
        lines.extend(crate::terminal_hyperlinks::adaptive_wrap_hyperlink_lines(
            &plain_hyperlink_lines(diagnostics),
            RtOptions::new(usize::from(width).max(/*other*/ 1))
                .initial_indent("  └ ".dim().into())
                .subsequent_indent("    ".into()),
        ));
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp.into());
        }
        lines
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::from("Failed to apply patch")];
        lines.extend(plain_lines(ansi_escape(&self.stderr).lines));
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp);
        }
        lines
    }

    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();

        // Failure title
        lines.push(Line::from("✘ Failed to apply patch".magenta().bold()));

        if !self.stderr.trim().is_empty() {
            let output = output_lines(
                Some(&CommandOutput::new(
                    /*exit_code*/ 1,
                    self.stderr.clone(),
                )),
                OutputLinesParams {
                    line_limit: TOOL_CALL_MAX_LINES,
                    only_err: true,
                    include_angle_pipe: true,
                    include_prefix: true,
                },
            );
            lines.extend(output.lines);
        }

        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp);
        }
        lines
    }
}

#[derive(Debug)]
pub(crate) struct ViewImageHistoryCell {
    activity_id: String,
    filename: String,
    path_label: String,
    link: Option<TerminalHyperlink>,
    timestamp: Option<String>,
}

impl ViewImageHistoryCell {
    pub(crate) fn with_timestamp(
        mut self,
        timestamp: Option<chrono::DateTime<chrono::Local>>,
    ) -> Self {
        if let Some(timestamp) = timestamp {
            self.timestamp = Some(crate::clock_format::format_local_timestamp(timestamp));
        }
        self
    }

    pub(crate) fn link_local_image(&mut self) {
        let start = "Viewed image ".len();
        let end = start + crate::width::display_width(&self.path_label);
        self.link = self
            .image_path()
            .and_then(|path| TerminalHyperlink::viewed_image(start..end, path.as_path()));
    }
}

impl HistoryCell for ViewImageHistoryCell {
    fn tool_call_labels(&self) -> Vec<String> {
        vec![match &self.timestamp {
            Some(timestamp) => format!("Viewed image {} • {timestamp}", self.path_label),
            None => format!("Viewed image {}", self.path_label),
        }]
    }

    fn activity_ids(&self) -> Vec<String> {
        vec![self.activity_id.clone()]
    }

    fn image_path(&self) -> Option<AbsolutePathBuf> {
        LegacyAppPathString::from_string(self.path_label.clone()).to_inferred_abs_path()
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let line = vec![
            "• ".dim(),
            "Viewed image ".bold(),
            self.filename.replace(['\n', '\r', '\t'], " ").dim(),
        ]
        .into();
        let mut lines = vec![truncate_line_with_ellipsis_if_overflow(
            line,
            width as usize,
        )];
        if let Some(timestamp) = &self.timestamp {
            lines.push(Line::from(vec!["  • ".dim(), timestamp.clone().dim()]));
        }
        lines
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = plain_hyperlink_lines(self.display_lines(width));
        if let (Some(line), Some(link)) = (lines.first_mut(), &self.link) {
            let start = crate::width::display_width("• Viewed image ");
            if start < line.width() {
                line.hyperlinks.push(link.with_columns(start..line.width()));
            }
        }
        lines
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        visible_lines(self.transcript_hyperlink_lines(width))
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        if width == 0 {
            return Vec::new();
        }
        let mut line = HyperlinkLine::new(Line::from(vec![
            "Viewed image ".bold(),
            self.path_label.clone().dim(),
        ]));
        line.hyperlinks.extend(self.link.iter().cloned());
        adaptive_wrap_hyperlink_lines(
            &[line],
            RtOptions::new(usize::from(width))
                .initial_indent("• ".dim().into())
                .subsequent_indent("  ".into()),
        )
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        vec![Line::from(format!("Viewed image {}", self.path_label))]
    }
}

pub(crate) fn new_view_image_tool_call(
    id: String,
    path: LegacyAppPathString,
) -> ViewImageHistoryCell {
    let filename = path
        .to_inferred_path_uri()
        .and_then(|path| path.basename())
        .unwrap_or_else(|| path.render_for_ui());
    ViewImageHistoryCell {
        activity_id: format!("image-view:{id}"),
        filename,
        path_label: path.into_string(),
        link: None,
        timestamp: Some(crate::clock_format::format_local_timestamp(
            chrono::Local::now(),
        )),
    }
}

pub(crate) fn new_image_generation_call(
    call_id: String,
    status: &str,
    revised_prompt: Option<String>,
    saved_path: Option<AbsolutePathBuf>,
) -> ImageGenerationHistoryCell {
    let detail = revised_prompt.unwrap_or_else(|| call_id.clone());
    let failed = status == "failed";
    let heading = if failed {
        vec!["✗ ".red().bold(), "Image generation failed".bold()].into()
    } else {
        vec!["• ".dim(), "Generated Image:".bold()].into()
    };
    let mut lines: Vec<Line<'static>> = vec![heading, vec!["  └ ".dim(), detail.dim()].into()];
    if let Some(saved_path) = &saved_path {
        let saved_path = Url::from_file_path(saved_path.as_path())
            .map(|url| url.to_string())
            .unwrap_or_else(|_| saved_path.display().to_string());
        lines.push(vec!["  └ ".dim(), "Saved to: ".dim(), saved_path.into()].into());
    }

    ImageGenerationHistoryCell {
        id: call_id,
        lines,
        failed,
        saved_path: saved_path.filter(|_| status == "completed"),
        timestamp: Some(crate::clock_format::format_local_timestamp(
            chrono::Local::now(),
        )),
    }
}

#[derive(Debug)]
pub(crate) struct ImageGenerationHistoryCell {
    pub(crate) id: String,
    failed: bool,
    lines: Vec<Line<'static>>,
    saved_path: Option<AbsolutePathBuf>,
    timestamp: Option<String>,
}

impl ImageGenerationHistoryCell {
    pub(crate) fn with_timestamp(
        mut self,
        timestamp: Option<chrono::DateTime<chrono::Local>>,
    ) -> Self {
        if let Some(timestamp) = timestamp {
            self.timestamp = Some(crate::clock_format::format_local_timestamp(timestamp));
        }
        self
    }

    /// A completed generation with a saved artifact belongs to the answer.
    pub(crate) fn is_turn_output(&self) -> bool {
        self.saved_path.is_some()
    }
}

/// Hold saved results until the turn can place them after the answer.
#[derive(Debug, Default)]
pub(crate) struct PendingGeneratedImages(HashMap<String, Vec<ImageGenerationHistoryCell>>);

impl PendingGeneratedImages {
    pub(crate) fn record(&mut self, turn_id: String, image: ImageGenerationHistoryCell) {
        let images = self.0.entry(turn_id).or_default();
        if let Some(existing) = images.iter_mut().find(|existing| existing.id == image.id) {
            *existing = image;
        } else {
            images.push(image);
        }
    }

    pub(crate) fn take(&mut self, turn_id: &str) -> Vec<ImageGenerationHistoryCell> {
        self.0.remove(turn_id).unwrap_or_default()
    }
}

impl HistoryCell for ImageGenerationHistoryCell {
    fn tool_call_labels(&self) -> Vec<String> {
        if self.is_turn_output() {
            return Vec::new();
        }
        let label = if self.failed {
            "Image generation failed"
        } else {
            "Generated image"
        };
        vec![label.into()]
    }

    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        let mut lines = self.lines.clone();
        if let Some(timestamp) = &self.timestamp {
            lines.push(Line::from(vec!["  • ".dim(), timestamp.clone().dim()]));
        }
        lines
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines =
            crate::terminal_hyperlinks::plain_hyperlink_lines(self.display_lines(width));
        let saved_line_index = lines
            .len()
            .saturating_sub(1 + usize::from(self.timestamp.is_some()));
        if let Some(path) = &self.saved_path
            && let Some(line) = lines.get_mut(saved_line_index)
            && let Some(span) = line.line.spans.last()
        {
            let width = crate::line_truncation::span_width(span);
            let start = line.line.width().saturating_sub(width);
            line.hyperlinks
                .push(crate::terminal_hyperlinks::TerminalHyperlink::document(
                    start..start + width,
                    crate::markdown_render::FileLink {
                        path: path.to_string_lossy().into_owned(),
                        location: None,
                    },
                ));
        }
        lines
    }

    fn compact_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let Some(path) = &self.saved_path else {
            return self.display_hyperlink_lines(width);
        };
        let mut line = HyperlinkLine::from(Line::from("  Generated image".dim()));
        line.hyperlinks.push(TerminalHyperlink::document(
            2..line.line.width(),
            crate::markdown_render::FileLink {
                path: path.to_string_lossy().into_owned(),
                location: None,
            },
        ));
        let mut lines = vec![line];
        if let Some(timestamp) = &self.timestamp {
            lines.push(Line::from(vec!["  • ".dim(), timestamp.clone().dim()]).into());
        }
        lines
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        self.display_hyperlink_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.lines.clone();
        if let Some(timestamp) = &self.timestamp {
            lines.push(Line::from(vec!["  • ".dim(), timestamp.clone().dim()]));
        }
        plain_lines(lines)
    }

    fn image_path(&self) -> Option<AbsolutePathBuf> {
        self.saved_path.clone()
    }
}
