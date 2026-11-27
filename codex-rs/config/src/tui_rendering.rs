//! TUI content rendering preferences, independent of animation effects.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Optional rich renderers. Disabled renderers preserve the original source.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, JsonSchema)]
#[serde(default)]
#[schemars(deny_unknown_fields)]
pub struct TuiRendering {
    /// Render Mermaid code blocks as diagrams.
    pub mermaid: bool,
    /// Render math expressions using Unicode notation.
    pub math: bool,
    /// Render pipe tables, including tables inside Markdown fences.
    pub tables: bool,
    /// Render Markdown bullets and task-list markers as Unicode symbols.
    pub lists: bool,
    /// Maximum transcript layouts retained by each transcript view.
    #[schemars(range(min = 1, max = 1024))]
    pub layout_cache_entries: usize,
    /// Approximate retained layout payload limit, in MiB, for each transcript view.
    #[schemars(range(min = 1, max = 128))]
    pub layout_cache_mib: usize,
}

impl Default for TuiRendering {
    fn default() -> Self {
        Self {
            mermaid: true,
            math: true,
            tables: true,
            lists: true,
            layout_cache_entries: 128,
            layout_cache_mib: 32,
        }
    }
}
