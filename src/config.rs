// SPDX-License-Identifier: MPL-2.0

use cosmic::cosmic_config::{self, cosmic_config_derive::CosmicConfigEntry, CosmicConfigEntry};

/// Persistent configuration for the Compact Launcher applet.
/// All fields mirror the GSettings keys in the GNOME extension counterpart.
#[derive(Debug, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 3]
pub struct Config {
    /// Size in pixels of the icon image inside each tile (16-128).
    pub icon_size: u32,
    /// Fixed width in pixels of each grid cell (48-256).
    pub cell_width: u32,
    /// Minimum height in pixels of each grid cell — icon + label (48-256).
    pub cell_height: u32,
    /// Horizontal gap in pixels between icon columns (0-64).
    pub col_spacing: u32,
    /// Vertical gap in pixels between icon rows (0-64).
    pub row_spacing: u32,
    /// Popup width in pixels (200-3840).
    pub popup_width: u32,
    /// Maximum popup height in pixels (200-2160).
    pub popup_height: u32,
    /// Pixel inset subtracted from the popup width when calculating column count (0-120).
    pub grid_margin_px: u32,
    /// Minimum number of icon columns regardless of popup size (1-20).
    pub min_cols: u32,
    /// Maximum number of columns; 0 = automatic (0-20).
    pub max_cols: u32,
    /// Whether to show the app name label below each icon tile.
    pub show_labels: bool,
    /// App name patterns (exact or glob with *) to hide from the grid.
    pub hidden_apps: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            icon_size: 48,
            cell_width: 80,
            cell_height: 88,
            col_spacing: 4,
            row_spacing: 4,
            popup_width: 360,
            popup_height: 800,
            grid_margin_px: 8,
            min_cols: 7,
            max_cols: 7,
            show_labels: true,
            hidden_apps: Vec::new(),
        }
    }
}
