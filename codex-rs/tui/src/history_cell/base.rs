//! Shared history-cell building blocks reused across transcript concerns.
//! Wrapped prefixes remain display-only while annotated rows retain their logical source.

use super::*;
use itertools::Itertools;

#[derive(Debug)]
pub(crate) struct PlainHistoryCell {
    pub(super) lines: Vec<Line<'static>>,
}

impl PlainHistoryCell {
    pub(crate) fn new(lines: Vec<Line<'static>>) -> Self {
        Self { lines }
    }

    /// Only invocation lines belong in a tool summary.
    pub(crate) fn with_tool_header(self, count: usize) -> LabeledToolCell {
        let label = self.lines.iter().take(count).join(" · ");
        self.with_tool_label(label)
    }

    pub(crate) fn with_tool_label(self, label: String) -> LabeledToolCell {
        LabeledToolCell {
            cell: self,
            label,
            timestamp: None,
        }
    }
}

impl HistoryCell for PlainHistoryCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines.clone()
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        plain_lines(self.lines.clone())
    }
}

#[derive(Debug)]
pub(crate) struct LabeledToolCell<C = PlainHistoryCell> {
    cell: C,
    label: String,
    timestamp: Option<String>,
}

impl<C: HistoryCell> LabeledToolCell<C> {
    pub(crate) fn with_timestamp(
        mut self,
        timestamp: Option<chrono::DateTime<chrono::Local>>,
    ) -> Self {
        self.timestamp = timestamp.map(crate::clock_format::format_local_timestamp);
        self
    }

    pub(crate) fn with_current_timestamp(self) -> Self {
        self.with_timestamp(Some(chrono::Local::now()))
    }

    fn timestamp_line(&self) -> Option<Line<'static>> {
        self.timestamp
            .as_ref()
            .map(|timestamp| Line::from(vec!["  \u{2022} ".dim(), timestamp.clone().dim()]))
    }
}

impl<C: HistoryCell> HistoryCell for LabeledToolCell<C> {
    fn tool_call_labels(&self) -> Vec<String> {
        vec![match &self.timestamp {
            Some(timestamp) => format!("{} \u{2022} {timestamp}", self.label),
            None => self.label.clone(),
        }]
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = self.cell.display_lines(width);
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp);
        }
        lines
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = self.cell.display_hyperlink_lines(width);
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp.into());
        }
        lines
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut lines = self.cell.transcript_hyperlink_lines(width);
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp.into());
        }
        lines
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.cell.raw_lines();
        if let Some(timestamp) = self.timestamp_line() {
            lines.push(timestamp);
        }
        lines
    }
}

#[derive(Debug)]
pub(crate) struct WebHyperlinkHistoryCell {
    lines: Vec<HyperlinkLine>,
}

impl WebHyperlinkHistoryCell {
    pub(crate) fn new(lines: Vec<Line<'static>>) -> Self {
        Self {
            lines: crate::terminal_hyperlinks::annotate_web_urls(lines),
        }
    }

    pub(crate) fn new_hyperlink_lines(lines: Vec<HyperlinkLine>) -> Self {
        Self { lines }
    }

    pub(crate) fn with_tool_header(self, count: usize) -> LabeledToolCell<Self> {
        let label = self
            .lines
            .iter()
            .take(count)
            .map(|line| &line.line)
            .join(" · ");
        self.with_tool_label(label)
    }

    pub(crate) fn with_tool_label(self, label: String) -> LabeledToolCell<Self> {
        LabeledToolCell {
            cell: self,
            label,
            timestamp: None,
        }
    }
}

impl HistoryCell for WebHyperlinkHistoryCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        self.lines.iter().map(|line| line.line.clone()).collect()
    }

    fn display_hyperlink_lines(&self, _width: u16) -> Vec<HyperlinkLine> {
        self.lines.clone()
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        self.display_hyperlink_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        plain_lines(self.lines.iter().map(|line| line.line.clone()))
    }
}
#[derive(Debug)]
pub(crate) struct PrefixedWrappedHistoryCell {
    text: Text<'static>,
    initial_prefix: Line<'static>,
    subsequent_prefix: Line<'static>,
}

impl PrefixedWrappedHistoryCell {
    pub(crate) fn new(
        text: impl Into<Text<'static>>,
        initial_prefix: impl Into<Line<'static>>,
        subsequent_prefix: impl Into<Line<'static>>,
    ) -> Self {
        Self {
            text: text.into(),
            initial_prefix: initial_prefix.into(),
            subsequent_prefix: subsequent_prefix.into(),
        }
    }
}

impl HistoryCell for PrefixedWrappedHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        visible_lines(self.display_hyperlink_lines(width))
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        if width == 0 {
            return Vec::new();
        }
        let opts = RtOptions::new(usize::from(width))
            .initial_indent(self.initial_prefix.clone())
            .subsequent_indent(self.subsequent_prefix.clone());
        crate::terminal_hyperlinks::adaptive_wrap_hyperlink_lines(
            &plain_hyperlink_lines(self.text.lines.clone()),
            opts,
        )
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        self.display_hyperlink_lines(width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        plain_lines(self.text.clone().lines)
    }
}
#[derive(Debug)]
pub(crate) struct CompositeHistoryCell {
    pub(super) parts: Vec<Box<dyn HistoryCell>>,
}

impl CompositeHistoryCell {
    pub(crate) fn new(parts: Vec<Box<dyn HistoryCell>>) -> Self {
        Self { parts }
    }
}

impl HistoryCell for CompositeHistoryCell {
    fn warning_entries(&self) -> Vec<WarningEntry> {
        self.parts
            .iter()
            .flat_map(|part| part.warning_entries())
            .collect()
    }

    fn live_raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for part in &self.parts {
            let part = part.live_raw_lines();
            if !part.is_empty() {
                if !lines.is_empty() {
                    lines.push(Line::default());
                }
                lines.extend(part);
            }
        }
        lines
    }

    fn warning_keys(&self) -> Vec<WarningKey<'_>> {
        self.parts
            .iter()
            .flat_map(|part| part.warning_keys())
            .collect()
    }

    fn compact_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut out = Vec::new();
        for part in &self.parts {
            let lines = part.compact_hyperlink_lines(width);
            if !lines.is_empty() {
                if !out.is_empty() {
                    out.push(HyperlinkLine::from(""));
                }
                out.extend(lines);
            }
        }
        out
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.display_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn display_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut out = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.display_hyperlink_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(HyperlinkLine::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn transcript_hyperlink_lines(&self, width: u16) -> Vec<HyperlinkLine> {
        let mut out = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.transcript_hyperlink_lines(width);
            if !lines.is_empty() {
                if !first {
                    out.push(HyperlinkLine::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut out: Vec<Line<'static>> = Vec::new();
        let mut first = true;
        for part in &self.parts {
            let mut lines = part.raw_lines();
            if !lines.is_empty() {
                if !first {
                    out.push(Line::from(""));
                }
                out.append(&mut lines);
                first = false;
            }
        }
        out
    }

    fn has_stable_transcript_height(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
