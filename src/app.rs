// SPDX-License-Identifier: MPL-2.0

use crate::config::Config;
use crate::fl;
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::{Alignment, Length, Limits, Size, Subscription};
use cosmic::iced::window::{self, Id};
use cosmic::prelude::*;
use cosmic::theme;
use cosmic::widget;
use std::path::PathBuf;

// ── Desktop entry ─────────────────────────────────────────────────────────────

/// A single installed application parsed from a .desktop file.
#[derive(Clone, Debug)]
pub struct AppEntry {
    /// Localised display name.
    pub name: String,
    /// Exec string with all %-field codes stripped.
    pub exec_clean: String,
    /// Icon name (theme) or absolute path; None = generic icon.
    pub icon: Option<String>,
}

// ── Popup view ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum PopupView {
    Grid,
    Settings,
}

// ── App model ─────────────────────────────────────────────────────────────────

pub struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    config: Config,
    config_handler: Option<cosmic_config::Config>,
    /// Installed applications, sorted alphabetically.
    apps: Vec<AppEntry>,
    /// Which sub-view is currently shown inside the popup.
    popup_view: PopupView,
    /// Live text in the "add hidden app pattern" field.
    hidden_app_input: String,
    /// Fingerprint of the app directories at the last load.
    /// Used by the periodic subscription to detect newly installed apps.
    apps_stamp: u64,
}

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // ── Popup lifecycle ───────────────────────────────────────────────────────
    TogglePopup,
    PopupClosed(Id),
    // ── External config update (from cosmic-settings-daemon) ─────────────────
    UpdateConfig(Config),
    // ── App grid ─────────────────────────────────────────────────────────────
    LaunchApp(usize),
    // ── Navigation ───────────────────────────────────────────────────────────
    SetView(PopupView),
    // ── Icon / tile settings ─────────────────────────────────────────────────
    SetIconSize(u32),
    SetCellWidth(u32),
    SetCellHeight(u32),
    SetColSpacing(u32),
    SetRowSpacing(u32),
    // ── Layout settings ───────────────────────────────────────────────────────
    SetPopupHeight(u32),
    SetMinCols(u32),
    SetMaxCols(u32),
    // ── Icon settings ─────────────────────────────────────────────────────────
    SetShowLabels(bool),
    // ── Filter settings ───────────────────────────────────────────────────────
    HiddenAppInput(String),
    AddHiddenApp,
    RemoveHiddenApp(String),
    // ── Reset ─────────────────────────────────────────────────────────────────
    ResetConfig,

    // ── Support ───────────────────────────────────────────────────────────────
    OpenDonationLink,

    // ── App-dir watcher ───────────────────────────────────────────────────────
    /// Fired every few seconds; reloads the app list if the directories changed.
    CheckApps,
}

// ── cosmic::Application impl ──────────────────────────────────────────────────

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "com.gitlab.gilsonfonsaca.compact-launcher";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(
        core: cosmic::Core,
        _flags: Self::Flags,
    ) -> (Self, Task<cosmic::Action<Self::Message>>) {
        let (config, config_handler) =
            match cosmic_config::Config::new(Self::APP_ID, Config::VERSION) {
                Ok(handler) => {
                    let config = match Config::get_entry(&handler) {
                        Ok(cfg) => cfg,
                        Err((_errors, cfg)) => cfg,
                    };
                    (config, Some(handler))
                }
                Err(_) => (Config::default(), None),
            };

        let apps = load_desktop_apps(&config.hidden_apps);
        let apps_stamp = app_dirs_stamp();

        let model = AppModel {
            core,
            popup: None,
            apps,
            apps_stamp,
            config,
            config_handler,
            popup_view: PopupView::Grid,
            hidden_app_input: String::new(),
        };

        (model, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    /// Panel icon button shown in the COSMIC panel.
    fn view(&self) -> Element<'_, Self::Message> {
        let suggested = self.core.applet.suggested_size(false);
        self.core
            .applet
            .icon_button_from_handle(
                widget::icon::from_name("com.gitlab.gilsonfonsaca.compact-launcher")
                    .symbolic(false)
                    .size(suggested.0)
                    .into(),
            )
            .on_press(Message::TogglePopup)
            .into()
    }

    /// Popup window content — rendered when the user clicks the panel button.
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let content = match &self.popup_view {
            PopupView::Grid => self.view_app_grid(),
            PopupView::Settings => self.view_settings(),
        };

        // Override the default 360px max_width so the popup auto-sizes
        // to the actual grid width (min_cols × cell_width + spacing).
        self.core
            .applet
            .popup_container(content)
            .limits(
                Limits::NONE
                    .min_height(1.0)
                    .min_width(100.0)
                    .max_width(4000.0)
                    .max_height(2000.0),
            )
            .into()
    }

    /// Watch for config file changes and periodically check for new apps.
    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            self.core()
                .watch_config::<Config>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
            cosmic::iced::time::every(std::time::Duration::from_secs(5))
                .map(|_| Message::CheckApps),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<cosmic::Action<Self::Message>> {
        match message {
            // ── Popup ─────────────────────────────────────────────────────────
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(
                            cosmic::surface::action::destroy_popup(p),
                        ),
                    ))
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    // Always reset to the grid view when reopening.
                    self.popup_view = PopupView::Grid;
                    let popup_action = cosmic::surface::action::app_popup::<Self>(
                        move |app| {
                            let w = app.grid_display_width() as u32;
                            let h = app.config.popup_height + 40;
                            let mut settings = app.core.applet.get_popup_settings(
                                app.core.main_window_id().unwrap(),
                                new_id,
                                Some((w, h)),
                                None,
                                None,
                            );
                            // get_popup_settings() hard-codes size_limits to
                            // min/max_width=360, which the Wayland compositor
                            // enforces and blocks all subsequent resize() calls.
                            // Override it to allow the dynamic grid width.
                            settings.positioner.size_limits = Limits::NONE
                                .min_height(1.0)
                                .min_width(100.0)
                                .max_width(2000.0)
                                .max_height(1200.0);
                            // Keep grab: true (the default) so that:
                            // 1. The Wayland compositor keeps an active input grab →
                            //    the popup is never auto-closed by idle/inactivity.
                            // 2. Clicking outside the popup still closes it normally.
                            // The old "grab: false" was needed for sliders (drag escapes
                            // popup bounds), but we now use spin_buttons, so no dragging.
                            settings
                        },
                        None,
                    );
                    cosmic::task::message(cosmic::Action::Cosmic(
                        cosmic::app::Action::Surface(popup_action),
                    ))
                };
            }

            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }

            // ── Config sync ───────────────────────────────────────────────────
            Message::UpdateConfig(config) => {
                let hidden_changed = config.hidden_apps != self.config.hidden_apps;
                self.config = config;
                if hidden_changed {
                    self.apps = load_desktop_apps(&self.config.hidden_apps);
                }
            }

            // ── App launch ────────────────────────────────────────────────────
            Message::LaunchApp(index) => {
                if let Some(app) = self.apps.get(index) {
                    let exec = app.exec_clean.clone();
                    let _ = tokio::process::Command::new("sh")
                        .arg("-c")
                        .arg(&exec)
                        .spawn();
                }
                // Close the popup after launching.
                return self.close_popup();
            }

            // ── Navigation ────────────────────────────────────────────────────
            Message::SetView(view) => {
                self.popup_view = view;
            }

            // ── Icon / tile settings ──────────────────────────────────────────
            Message::SetIconSize(v) => {
                self.config.icon_size = v.clamp(16, 96);
                self.save_config();
            }
            Message::SetCellWidth(v) => {
                self.config.cell_width = v.clamp(48, 128);
                self.save_config();
                return self.apply_popup_resize();
            }
            Message::SetCellHeight(v) => {
                self.config.cell_height = v.clamp(48, 256);
                self.save_config();
            }
            Message::SetColSpacing(v) => {
                self.config.col_spacing = v.clamp(0, 64);
                self.save_config();
                return self.apply_popup_resize();
            }
            Message::SetRowSpacing(v) => {
                self.config.row_spacing = v.clamp(0, 64);
                self.save_config();
            }

            // ── Layout settings ───────────────────────────────────────────────
            Message::SetPopupHeight(v) => {
                self.config.popup_height = v.clamp(200, 900);
                self.save_config();
                return self.apply_popup_resize();
            }
            Message::SetMinCols(v) => {
                self.config.min_cols = v.clamp(1, 8);
                self.save_config();
                return self.apply_popup_resize();
            }
            Message::SetMaxCols(v) => {
                self.config.max_cols = v.clamp(0, 8);
                self.save_config();
                return self.apply_popup_resize();
            }

            // ── Icon settings ─────────────────────────────────────────────────
            Message::SetShowLabels(v) => {
                self.config.show_labels = v;
                self.save_config();
            }

            // ── Filter settings ───────────────────────────────────────────────
            Message::HiddenAppInput(text) => {
                self.hidden_app_input = text;
            }
            Message::AddHiddenApp => {
                let pattern = self.hidden_app_input.trim().to_string();
                if !pattern.is_empty() && !self.config.hidden_apps.contains(&pattern) {
                    self.config.hidden_apps.push(pattern);
                    self.save_config();
                    self.apps = load_desktop_apps(&self.config.hidden_apps);
                }
                self.hidden_app_input.clear();
            }
            Message::RemoveHiddenApp(pattern) => {
                self.config.hidden_apps.retain(|p| p != &pattern);
                self.save_config();
                self.apps = load_desktop_apps(&self.config.hidden_apps);
            }

            // ── Reset ─────────────────────────────────────────────────────────
            Message::ResetConfig => {
                self.config = Config::default();
                self.save_config();
                self.apps = load_desktop_apps(&self.config.hidden_apps);
            }

            // ── Support ───────────────────────────────────────────────────────
            Message::OpenDonationLink => {
                let _ = std::process::Command::new("xdg-open")
                    .arg("https://www.buymeacoffee.com/Gilsonf")
                    .spawn();
            }

            // ── App-dir watcher ───────────────────────────────────────────────
            Message::CheckApps => {
                let new_stamp = app_dirs_stamp();
                if new_stamp != self.apps_stamp {
                    self.apps_stamp = new_stamp;
                    self.apps = load_desktop_apps(&self.config.hidden_apps);
                }
            }
        }

        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

impl AppModel {
    // ── Config persistence ────────────────────────────────────────────────────

    fn save_config(&self) {
        if let Some(handler) = &self.config_handler {
            let _ = self.config.write_entry(handler);
        }
    }

    fn close_popup(&mut self) -> Task<cosmic::Action<Message>> {
        if let Some(p) = self.popup.take() {
            cosmic::task::message(cosmic::Action::Cosmic(
                cosmic::app::Action::Surface(
                    cosmic::surface::action::destroy_popup(p),
                ),
            ))
        } else {
            Task::none()
        }
    }

    // ── Column / width calculation ────────────────────────────────────────────

    /// Returns the number of columns to display.
    /// `min_cols` is the direct column count; `max_cols` optionally caps it.
    fn calculate_columns(&self) -> usize {
        let cols = if self.config.max_cols > 0 {
            self.config.min_cols.min(self.config.max_cols)
        } else {
            self.config.min_cols
        };
        cols.max(1) as usize
    }

    /// Pixel width the scrollable (and thus the popup) needs to fit the grid.
    ///
    /// Each button = cell_width + 2×button_padding(4px) = cell_width + 8.
    /// Column padding is 8px each side = 16px total inner margin.
    fn grid_display_width(&self) -> f32 {
        let cols = self.calculate_columns() as u32;
        let tile_w = self.config.cell_width + 8; // tile + button padding
        let spacing = if cols > 1 {
            (cols - 1) * self.config.col_spacing
        } else {
            0
        };
        (cols * tile_w + spacing + 16) as f32 // +16 = 8px inner padding each side
    }

    /// Return a Task that resizes the popup window to the current grid dimensions.
    /// Called after any setting that changes the popup width or height.
    fn apply_popup_resize(&self) -> Task<cosmic::Action<Message>> {
        if let Some(popup_id) = self.popup {
            let w = self.grid_display_width();
            // +40 covers header (≈28px) + divider (2px) + container border/padding (≈10px)
            let h = self.config.popup_height as f32 + 40.0;
            window::resize(popup_id, Size::new(w, h))
        } else {
            Task::none()
        }
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    /// Main popup view: app grid with a settings toggle in the header.
    fn view_app_grid(&self) -> Element<'_, Message> {
        let header = widget::Row::new()
            .push(
                widget::text(fl!("app-grid-title"))
                    .size(16)
                    .width(Length::Fill),
            )
            .push(
                widget::button::icon(
                    widget::icon::from_name("preferences-system-symbolic"),
                )
                .on_press(Message::SetView(PopupView::Settings)),
            )
            .align_y(Alignment::Center)
            .padding([8, 12, 4, 12]);

        let grid = self.build_app_grid();

        widget::Column::new()
            .push(header)
            .push(widget::divider::horizontal::default())
            .push(grid)
            .width(Length::Fixed(self.grid_display_width()))
            .into()
    }

    /// Build the scrollable icon grid for the installed applications.
    fn build_app_grid(&self) -> Element<'_, Message> {
        if self.apps.is_empty() {
            return widget::container(
                widget::text(fl!("no-apps-found")).width(Length::Fill),
            )
            .padding(24)
            .into();
        }

        let cols = self.calculate_columns();
        let scroll_height = self.config.popup_height as f32;
        let cell_w = self.config.cell_width as f32;
        let cell_h = self.config.cell_height as f32;

        let rows: Vec<Element<'_, Message>> = self
            .apps
            .chunks(cols)
            .enumerate()
            .map(|(row_i, chunk)| {
                let mut tiles: Vec<Element<'_, Message>> = chunk
                    .iter()
                    .enumerate()
                    .map(|(col_i, app)| {
                        let app_idx = row_i * cols + col_i;
                        self.app_tile(app, app_idx)
                    })
                    .collect();

                // Pad the last row with invisible spacers matching the
                // button width (cell_w + 8px padding on each side).
                while tiles.len() < cols {
                    tiles.push(
                        widget::space()
                            .width(Length::Fixed(cell_w + 8.0))
                            .height(Length::Fixed(cell_h))
                            .into(),
                    );
                }

                widget::Row::with_children(tiles)
                    .spacing(self.config.col_spacing)
                    .into()
            })
            .collect();

        widget::scrollable(
            widget::Column::with_children(rows)
                .spacing(self.config.row_spacing)
                .padding(8),
        )
        .height(Length::Fixed(scroll_height))
        .width(Length::Fixed(self.grid_display_width()))
        .into()
    }

    /// Build a single application tile (icon + label) as a pressable button.
    fn app_tile<'a>(&'a self, app: &'a AppEntry, index: usize) -> Element<'a, Message> {
        let icon_size = self.config.icon_size as u16;
        let cell_w = self.config.cell_width as f32;
        let cell_h = self.config.cell_height as f32;

        let icon = match &app.icon {
            Some(name) if name.starts_with('/') => {
                // Absolute path: load directly and let content_fit handle scaling.
                widget::icon::from_path(PathBuf::from(name))
                    .icon()
                    .size(icon_size)
                    .content_fit(cosmic::iced::ContentFit::Contain)
            }
            Some(name) => {
                // Theme name: set size BEFORE .icon() so the freedesktop lookup
                // fetches the correctly-sized asset instead of scaling a small one up.
                widget::icon::from_name(name.as_str())
                    .size(icon_size)        // lookup hint → finds 48 px asset, not 16 px
                    .prefer_svg(true)       // prefer crisp SVG over raster PNG when available
                    .icon()
            }
            None => widget::icon::from_name("application-x-executable")
                .size(icon_size)
                .prefer_svg(true)
                .icon(),
        };

        let mut tile = widget::Column::new()
            .push(icon)
            .align_x(Alignment::Center)
            .spacing(4)
            .width(Length::Fixed(cell_w))
            .height(Length::Fixed(cell_h));

        if self.config.show_labels {
            tile = tile.push(
                widget::text(app.name.as_str())
                    .size(11)
                    .center()
                    .width(Length::Fixed(cell_w)),
            );
        }

        widget::button::custom(tile)
            .on_press(Message::LaunchApp(index))
            .padding(4)
            .class(theme::Button::Text)
            .into()
    }

    // ── Settings view ─────────────────────────────────────────────────────────

    fn view_settings(&self) -> Element<'_, Message> {
        let header = widget::Row::new()
            .push(
                widget::button::icon(
                    widget::icon::from_name("go-previous-symbolic"),
                )
                .on_press(Message::SetView(PopupView::Grid)),
            )
            .push(widget::text(fl!("settings-title")).size(16))
            .align_y(Alignment::Center)
            .spacing(4)
            .padding([8, 12, 4, 12]);

        let scroll_height = self.config.popup_height as f32;

        let settings_content = widget::Column::new()
            .push(self.settings_icons_section())
            .push(self.settings_layout_section())
            .push(self.settings_filters_section())
            .push(self.settings_reset_section())
            .push(self.settings_donate_section())
            .spacing(16)
            .padding([8, 0]);

        widget::Column::new()
            .push(header)
            .push(widget::divider::horizontal::default())
            .push(
                widget::scrollable(settings_content)
                    .height(Length::Fixed(scroll_height))
                    .width(Length::Fixed(self.grid_display_width())),
            )
            .width(Length::Fixed(self.grid_display_width()))
            .into()
    }

    fn settings_icons_section(&self) -> Element<'_, Message> {
        widget::settings::section()
            .title(fl!("section-icons"))
            .add(widget::settings::item(
                fl!("show-labels"),
                widget::toggler(self.config.show_labels)
                    .on_toggle(Message::SetShowLabels),
            ))
            .add(widget::settings::item(
                fl!("icon-size"),
                widget::spin_button(
                    fl!("px", value = self.config.icon_size),
                    fl!("icon-size"),
                    self.config.icon_size,
                    4u32, 16u32, 96u32,
                    Message::SetIconSize,
                ),
            ))
            .add(widget::settings::item(
                fl!("cell-width"),
                widget::spin_button(
                    fl!("px", value = self.config.cell_width),
                    fl!("cell-width"),
                    self.config.cell_width,
                    4u32, 48u32, 128u32,
                    Message::SetCellWidth,
                ),
            ))
            .add(widget::settings::item(
                fl!("cell-height"),
                widget::spin_button(
                    fl!("px", value = self.config.cell_height),
                    fl!("cell-height"),
                    self.config.cell_height,
                    4u32, 48u32, 256u32,
                    Message::SetCellHeight,
                ),
            ))
            .add(widget::settings::item(
                fl!("col-spacing"),
                widget::spin_button(
                    fl!("px", value = self.config.col_spacing),
                    fl!("col-spacing"),
                    self.config.col_spacing,
                    1u32, 0u32, 64u32,
                    Message::SetColSpacing,
                ),
            ))
            .add(widget::settings::item(
                fl!("row-spacing"),
                widget::spin_button(
                    fl!("px", value = self.config.row_spacing),
                    fl!("row-spacing"),
                    self.config.row_spacing,
                    1u32, 0u32, 64u32,
                    Message::SetRowSpacing,
                ),
            ))
            .into()
    }

    fn settings_layout_section(&self) -> Element<'_, Message> {
        let max_cols_label = if self.config.max_cols == 0 {
            fl!("count-auto")
        } else {
            self.config.max_cols.to_string()
        };

        widget::settings::section()
            .title(fl!("section-layout"))
            .add(widget::settings::item(
                fl!("min-cols"),
                widget::spin_button(
                    self.config.min_cols.to_string(),
                    fl!("min-cols"),
                    self.config.min_cols,
                    1u32, 1u32, 8u32,
                    Message::SetMinCols,
                ),
            ))
            .add(widget::settings::item(
                fl!("max-cols"),
                widget::spin_button(
                    max_cols_label,
                    fl!("max-cols"),
                    self.config.max_cols,
                    1u32, 0u32, 8u32,
                    Message::SetMaxCols,
                ),
            ))
            .add(widget::settings::item(
                fl!("popup-height"),
                widget::spin_button(
                    fl!("px", value = self.config.popup_height),
                    fl!("popup-height"),
                    self.config.popup_height,
                    20u32, 200u32, 900u32,
                    Message::SetPopupHeight,
                ),
            ))
            .into()
    }

    fn settings_filters_section(&self) -> Element<'_, Message> {
        let mut section = widget::settings::section().title(fl!("section-filters"));

        // Text input for adding a new hidden-app pattern
        let input_row = widget::Row::new()
            .push(
                widget::text_input(
                    fl!("hidden-app-placeholder"),
                    self.hidden_app_input.as_str(),
                )
                .on_input(Message::HiddenAppInput)
                .on_submit(|_| Message::AddHiddenApp)
                .width(Length::Fill),
            )
            .push(widget::button::standard("+").on_press(Message::AddHiddenApp))
            .spacing(8)
            .align_y(Alignment::Center);

        section = section.add(widget::settings::item(fl!("hidden-apps"), input_row));

        // One deletable row per existing pattern
        for pattern in &self.config.hidden_apps {
            let p = pattern.clone();
            let row = widget::Row::new()
                .push(widget::text(pattern.as_str()).width(Length::Fill))
                .push(
                    widget::button::icon(
                        widget::icon::from_name("user-trash-symbolic"),
                    )
                    .on_press(Message::RemoveHiddenApp(p)),
                )
                .align_y(Alignment::Center);

            section = section.add(widget::container(row).padding([0, 8]));
        }

        section.into()
    }

    fn settings_reset_section(&self) -> Element<'_, Message> {
        widget::settings::section()
            .add(widget::settings::item(
                fl!("reset-defaults"),
                widget::button::destructive(fl!("reset-defaults"))
                    .on_press(Message::ResetConfig),
            ))
            .into()
    }

    fn settings_donate_section(&self) -> Element<'_, Message> {
        widget::container(
            widget::Row::new()
                .push(widget::space::horizontal())
                .push(
                    widget::button::link(fl!("donate-label"))
                        .on_press(Message::OpenDonationLink),
                )
                .push(widget::space::horizontal()),
        )
        .padding([4, 0])
        .into()
    }
}

// ── Desktop file loading ──────────────────────────────────────────────────────

/// Scans standard XDG application directories (including Flatpak), parses
/// .desktop files, filters hidden apps, and returns entries sorted alphabetically.
fn load_desktop_apps(hidden_patterns: &[String]) -> Vec<AppEntry> {
    let search_dirs = app_search_dirs();

    let mut apps: Vec<AppEntry> = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for dir in &search_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }
            if let Some(app) = parse_desktop_file(&path, hidden_patterns) {
                if seen_names.insert(app.name.clone()) {
                    apps.push(app);
                }
            }
        }
    }

    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// Parses a single .desktop file and returns an `AppEntry` if the file
/// represents a visible application not matching any hidden pattern.
fn parse_desktop_file(path: &PathBuf, hidden_patterns: &[String]) -> Option<AppEntry> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut name = String::new();
    let mut exec = String::new();
    let mut icon: Option<String> = None;
    let mut app_type = String::new();
    let mut no_display = false;
    let mut hidden = false;
    let mut in_section = false;

    for line in content.lines() {
        let line = line.trim();

        if line == "[Desktop Entry]" {
            in_section = true;
            continue;
        }
        // Stop parsing on any other section header.
        if line.starts_with('[') {
            if in_section {
                break;
            }
            continue;
        }
        if !in_section {
            continue;
        }

        if let Some(v) = line.strip_prefix("Name=") {
            if name.is_empty() {
                name = v.to_string();
            }
        } else if let Some(v) = line.strip_prefix("Exec=") {
            exec = v.to_string();
        } else if let Some(v) = line.strip_prefix("Icon=") {
            icon = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("Type=") {
            app_type = v.to_string();
        } else if line == "NoDisplay=true" {
            no_display = true;
        } else if line == "Hidden=true" {
            hidden = true;
        }
    }

    if no_display || hidden || app_type != "Application" || name.is_empty() || exec.is_empty() {
        return None;
    }

    // Apply hidden-app glob filters.
    if hidden_patterns
        .iter()
        .any(|p| matches_hidden_pattern(&name, p))
    {
        return None;
    }

    Some(AppEntry {
        name,
        exec_clean: clean_exec(&exec),
        icon,
    })
}

/// Strips all %-field codes from a desktop Exec value.
fn clean_exec(exec: &str) -> String {
    exec.replace("%f", "")
        .replace("%F", "")
        .replace("%u", "")
        .replace("%U", "")
        .replace("%d", "")
        .replace("%D", "")
        .replace("%n", "")
        .replace("%N", "")
        .replace("%i", "")
        .replace("%c", "")
        .replace("%k", "")
        .replace("%v", "")
        .replace("%m", "")
        .trim()
        .to_string()
}

/// Returns all directories that are scanned for .desktop files.
/// Mirrors the lookup order used by `load_desktop_apps`.
fn app_search_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
        dirs.push(std::path::PathBuf::from(xdg_data).join("applications"));
    } else if let Ok(home) = std::env::var("HOME") {
        dirs.push(std::path::PathBuf::from(home).join(".local/share/applications"));
    }

    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in xdg_data_dirs.split(':') {
        dirs.push(std::path::PathBuf::from(dir).join("applications"));
    }

    dirs
}

/// Computes a cheap fingerprint of the application directories: the sum of
/// each directory's mtime (in seconds) plus its .desktop file count.
///
/// If any directory gains, loses, or updates a .desktop file the fingerprint
/// changes, triggering a reload of the app list.
fn app_dirs_stamp() -> u64 {
    app_search_dirs()
        .into_iter()
        .filter_map(|dir| {
            let mtime = std::fs::metadata(&dir)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let count = std::fs::read_dir(&dir)
                .map(|rd| rd.flatten().count() as u64)
                .unwrap_or(0);
            Some(mtime.wrapping_add(count))
        })
        .fold(0u64, u64::wrapping_add)
}

/// Case-insensitive glob match where `*` is the only wildcard (matches any sequence).
/// Mirrors the GNOME extension's `_matchesGlob()` helper.
fn matches_hidden_pattern(name: &str, pattern: &str) -> bool {
    let p = pattern.trim().to_lowercase();
    let n = name.to_lowercase();

    if p.is_empty() {
        return false;
    }
    if !p.contains('*') {
        return n == p;
    }

    // Split on '*' and verify each segment appears in order in the name.
    let parts: Vec<&str> = p.split('*').collect();
    let mut remaining = n.as_str();

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            // First segment must match at the very start.
            if !remaining.starts_with(part) {
                return false;
            }
            remaining = &remaining[part.len()..];
        } else {
            // Subsequent segments: find them anywhere in what's left.
            if let Some(pos) = remaining.find(part) {
                remaining = &remaining[pos + part.len()..];
            } else {
                return false;
            }
        }
    }

    true
}
