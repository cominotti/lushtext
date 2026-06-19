// SPDX-License-Identifier: GPL-3.0-or-later

//! Private implementation for the preferences dialog.
//!
//! Binds GSettings keys to Adwaita preference rows (switches, combos, spin)
//! using two-way `Settings::bind()`. The color scheme row and font button
//! require manual wiring because their value types don't map directly to
//! GSettings string/bool keys, and the transparency control formats a double
//! setting into the percentage label shown in its row suffix.

use crate::config::keys;
use crate::services::{format_upgrade, json_store};
use crate::ui::accessibility;
use crate::ui::sidebar::WorkspaceSidebarWidthPreset;
use glib::subclass::prelude::ObjectSubclassIsExt;
use glib::value::ToValue;
use gtk_lush_tasks::spawn_blocking_then_weak;
use gtk4::{self, CompositeTemplate, gio, glib};
use libadwaita::prelude::*;
use libadwaita::subclass::prelude::*;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// Maximum rows rendered per metadata group before showing an omitted-count row.
const DATA_DETAILS_MAX_ROWS_PER_GROUP: usize = 32;
/// Minimum time a successful fast scan remains visibly "checking" in Preferences.
///
/// One second is long enough for users to perceive that Refresh did work while
/// staying short enough that genuinely slow scans are not delayed further.
const DATA_SCAN_MINIMUM_VISIBLE_DURATION: Duration = Duration::from_secs(1);

/// Background conversion result returned to the Preferences Data page on the GTK main thread.
enum DataConvertWorkerResult {
    NoConvert(format_upgrade::FormatPlan),
    Applied {
        plan: format_upgrade::FormatPlan,
        result: Result<format_upgrade::FormatApplyOutcome, String>,
    },
}

/// Presentation policy for a Data page scan result.
#[derive(Clone, Copy)]
enum DataScanPresentation {
    /// Initial and post-convert scans should reveal actionable state as soon as it is ready.
    Immediate,
    /// Manual Refresh keeps the checking state visible long enough to prove the click did work.
    VisibleDwell,
}

// CompositeTemplate loads preferences.ui from the compiled GResource; each
// #[template_child] field is bound to the widget with the matching template ID.
#[derive(CompositeTemplate)]
#[template(resource = "/dev/cominotti/lushtext/ui/preferences.ui")]
pub struct LushtextPreferences {
    #[template_child]
    pub style_scheme_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub workspace_sidebar_width_row: TemplateChild<libadwaita::ComboRow>,
    #[template_child]
    pub use_system_font_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub custom_font_row: TemplateChild<libadwaita::ActionRow>,
    #[template_child]
    pub font_button: TemplateChild<gtk4::FontDialogButton>,
    #[template_child]
    pub transparency_row: TemplateChild<libadwaita::ActionRow>,
    #[template_child]
    pub transparency_button: TemplateChild<gtk4::MenuButton>,
    /// Percentage suffix for the tab-content opacity slider.
    #[template_child]
    pub transparency_label: TemplateChild<gtk4::Label>,
    /// Slider adjustment for the persisted opacity value and visible percentage projection.
    #[template_child]
    pub transparency_adjustment: TemplateChild<gtk4::Adjustment>,
    #[template_child]
    pub focus_mode_target_columns_row: TemplateChild<libadwaita::SpinRow>,
    #[template_child]
    pub focus_mode_typewriter_scrolling_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub editorconfig_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub word_wrap_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub tab_width_row: TemplateChild<libadwaita::SpinRow>,
    #[template_child]
    pub insert_spaces_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub show_line_numbers_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub highlight_line_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub minimap_group: TemplateChild<libadwaita::PreferencesGroup>,
    #[template_child]
    pub show_minimap_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub minimap_long_line_markers_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub bookmark_gutter_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub workspace_auto_collapse_row: TemplateChild<libadwaita::SwitchRow>,
    #[template_child]
    pub workspace_empty_folder_lookahead_cap_row: TemplateChild<libadwaita::SpinRow>,
    /// Status row summarizing the latest app-data format scan.
    #[template_child]
    pub data_status_row: TemplateChild<libadwaita::ActionRow>,
    /// Success indicator shown only after the latest scan proves current data.
    #[template_child]
    pub data_current_indicator: TemplateChild<gtk4::Image>,
    /// Button that reruns the read-only app-data format scan.
    #[template_child]
    pub data_scan_button: TemplateChild<gtk4::Button>,
    /// Group containing Data page actions; hidden when no real action is available.
    #[template_child]
    pub data_actions_group: TemplateChild<libadwaita::PreferencesGroup>,
    /// Row containing the Convert action when the last scan found a supported upgrade.
    #[template_child]
    pub data_convert_row: TemplateChild<libadwaita::ActionRow>,
    /// Button that applies supported conversions after rescanning app data.
    #[template_child]
    pub data_convert_button: TemplateChild<gtk4::Button>,
    /// Group that hosts the bounded per-file format details list.
    #[template_child]
    pub data_details_group: TemplateChild<libadwaita::PreferencesGroup>,

    pub settings: gio::Settings,
    /// Scroll-contained list of metadata details for the Data page.
    pub data_details_list: gtk4::ListBox,
    /// Whether the last completed scan exposed a supported Convert action.
    pub data_last_scan_offers_convert: Cell<bool>,
    /// Whether a scan or conversion command is already running.
    pub data_operation_inflight: Cell<bool>,
    /// Throttles repeated Data-page format scan/apply outcome announcements.
    pub data_announcement_throttler: accessibility::AnnouncementThrottler,
}

impl Default for LushtextPreferences {
    fn default() -> Self {
        Self {
            style_scheme_row: TemplateChild::default(),
            workspace_sidebar_width_row: TemplateChild::default(),
            editorconfig_row: TemplateChild::default(),
            use_system_font_row: TemplateChild::default(),
            custom_font_row: TemplateChild::default(),
            font_button: TemplateChild::default(),
            transparency_row: TemplateChild::default(),
            transparency_button: TemplateChild::default(),
            transparency_label: TemplateChild::default(),
            transparency_adjustment: TemplateChild::default(),
            focus_mode_target_columns_row: TemplateChild::default(),
            focus_mode_typewriter_scrolling_row: TemplateChild::default(),
            word_wrap_row: TemplateChild::default(),
            tab_width_row: TemplateChild::default(),
            insert_spaces_row: TemplateChild::default(),
            show_line_numbers_row: TemplateChild::default(),
            highlight_line_row: TemplateChild::default(),
            minimap_group: TemplateChild::default(),
            show_minimap_row: TemplateChild::default(),
            minimap_long_line_markers_row: TemplateChild::default(),
            bookmark_gutter_row: TemplateChild::default(),
            workspace_auto_collapse_row: TemplateChild::default(),
            workspace_empty_folder_lookahead_cap_row: TemplateChild::default(),
            data_status_row: TemplateChild::default(),
            data_current_indicator: TemplateChild::default(),
            data_scan_button: TemplateChild::default(),
            data_actions_group: TemplateChild::default(),
            data_convert_row: TemplateChild::default(),
            data_convert_button: TemplateChild::default(),
            data_details_group: TemplateChild::default(),
            settings: gio::Settings::new(crate::config::APP_ID),
            data_details_list: gtk4::ListBox::new(),
            data_last_scan_offers_convert: Cell::new(false),
            data_operation_inflight: Cell::new(false),
            data_announcement_throttler: accessibility::AnnouncementThrottler::default(),
        }
    }
}

#[glib::object_subclass]
// ObjectSubclass registers this Rust struct as the GLib runtime type;
// ObjectImpl below owns lifecycle hooks after GTK initializes template children.
impl ObjectSubclass for LushtextPreferences {
    const NAME: &'static str = "LushtextPreferences";
    type Type = super::LushtextPreferences;
    type ParentType = libadwaita::PreferencesDialog;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for LushtextPreferences {
    fn constructed(&self) {
        self.parent_constructed();

        let s = &self.settings;

        // GSettings bind() creates a live two-way sync between settings keys
        // and widget properties. DEFAULT flags (the default) means changes to
        // either side automatically propagate to the other.
        s.bind(keys::USE_EDITORCONFIG, &*self.editorconfig_row, "active")
            .build();
        s.bind(keys::WORD_WRAP, &*self.word_wrap_row, "active")
            .build();
        s.bind(
            keys::SHOW_LINE_NUMBERS,
            &*self.show_line_numbers_row,
            "active",
        )
        .build();
        s.bind(
            keys::HIGHLIGHT_CURRENT_LINE,
            &*self.highlight_line_row,
            "active",
        )
        .build();
        s.bind(keys::SHOW_MINIMAP, &*self.show_minimap_row, "active")
            .build();
        s.bind(
            keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE,
            &*self.minimap_long_line_markers_row,
            "active",
        )
        .build();
        s.bind(
            keys::BOOKMARK_GUTTER_VISIBLE,
            &*self.bookmark_gutter_row,
            "active",
        )
        .build();
        s.bind(keys::INSERT_SPACES, &*self.insert_spaces_row, "active")
            .build();
        s.bind(keys::USE_SYSTEM_FONT, &*self.use_system_font_row, "active")
            .build();
        s.bind(keys::TAB_WIDTH, &self.tab_width_row.adjustment(), "value")
            .build();
        s.bind(
            keys::WORKSPACE_AUTO_COLLAPSE,
            &*self.workspace_auto_collapse_row,
            "active",
        )
        .build();
        s.bind(
            keys::WORKSPACE_EMPTY_FOLDER_LOOKAHEAD_CAP,
            &self.workspace_empty_folder_lookahead_cap_row.adjustment(),
            "value",
        )
        .build();

        s.bind(keys::USE_SYSTEM_FONT, &*self.custom_font_row, "sensitive")
            .flags(gio::SettingsBindFlags::GET | gio::SettingsBindFlags::INVERT_BOOLEAN)
            .build();
        s.bind(
            keys::TAB_CONTENT_OPACITY,
            &*self.transparency_adjustment,
            "value",
        )
        .build();
        s.bind(
            keys::FOCUS_MODE_TARGET_COLUMNS,
            &self.focus_mode_target_columns_row.adjustment(),
            "value",
        )
        .build();
        s.bind(
            keys::FOCUS_MODE_TYPEWRITER_SCROLLING,
            &*self.focus_mode_typewriter_scrolling_row,
            "active",
        )
        .build();

        self.setup_color_scheme_row();
        self.setup_workspace_sidebar_width_row();
        self.setup_font_button();
        self.setup_transparency_row();
        self.setup_data_page();
        self.apply_accessibility_metadata();
    }
}

impl LushtextPreferences {
    /// Keep numeric Adwaita preference rows discoverable as composite groups.
    /// Their internal child owns the `SpinButton` role, so the row itself must
    /// avoid the weaker presentation role that hides the control grouping.
    fn apply_accessibility_metadata(&self) {
        accessibility::set_role(&*self.tab_width_row, gtk4::AccessibleRole::Group);
        accessibility::set_role(
            &*self.focus_mode_target_columns_row,
            gtk4::AccessibleRole::Group,
        );
        accessibility::set_role(
            &*self.workspace_empty_folder_lookahead_cap_row,
            gtk4::AccessibleRole::Group,
        );
        accessibility::set_labelled_description(
            &*self.transparency_button,
            "Background opacity",
            "Adjust editor and Markdown preview document-surface opacity",
        );
        accessibility::set_has_popup(&*self.transparency_button, true);
        accessibility::set_labelled_description(
            &*self.data_status_row,
            "App data format status",
            "Latest scan result for persisted LushText app data",
        );
        accessibility::set_labelled_description(
            &*self.data_scan_button,
            "Rescan app data formats",
            "Run a read-only scan of persisted LushText app data",
        );
        accessibility::set_label(
            &*self.data_current_indicator,
            "App data format verified current",
        );
        accessibility::set_labelled_description(
            &*self.data_convert_button,
            "Convert supported older app data",
            "Update supported older LushText app data after a fresh scan",
        );
        accessibility::set_role(&self.data_details_list, gtk4::AccessibleRole::List);
        accessibility::set_labelled_description(
            &self.data_details_list,
            "App data format details",
            "Bounded list of app data files and planned format actions",
        );
        self.refresh_data_accessibility_state();
    }

    /// Build and wire the Preferences > Data page.
    fn setup_data_page(&self) {
        self.data_details_list
            .set_selection_mode(gtk4::SelectionMode::None);
        self.data_details_list.add_css_class("boxed-list");

        let scroll = gtk4::ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(240)
            .child(&self.data_details_list)
            .build();
        self.data_details_group.add(&scroll);

        let prefs_weak = self.obj().downgrade();
        // GObject signals are GTK's observer pattern: the button emits
        // "clicked", and this closure upgrades the weak dialog reference before
        // changing UI state.
        self.data_scan_button.connect_clicked(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs
                    .imp()
                    .run_data_scan(DataScanPresentation::VisibleDwell);
            }
        });

        let prefs_weak = self.obj().downgrade();
        self.data_convert_button.connect_clicked(move |_| {
            if let Some(prefs) = prefs_weak.upgrade() {
                prefs.imp().run_data_convert();
            }
        });

        self.run_data_scan(DataScanPresentation::Immediate);
    }

    /// Run a Data-page scan on a background thread.
    fn run_data_scan(&self, presentation: DataScanPresentation) {
        if self.data_operation_inflight.replace(true) {
            return;
        }
        self.show_data_operation_in_progress("Verifying app data formats");

        let prefs = self.obj().clone();
        let scan_started_at = Instant::now();
        let minimum_visible_duration = match presentation {
            DataScanPresentation::Immediate => Duration::ZERO,
            DataScanPresentation::VisibleDwell => DATA_SCAN_MINIMUM_VISIBLE_DURATION,
        };
        spawn_blocking_then_weak(
            &prefs,
            || {
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                format_upgrade::build_plan(&inventory)
            },
            move |prefs, plan| {
                let imp = prefs.imp();
                let remaining = minimum_visible_duration.saturating_sub(scan_started_at.elapsed());
                if remaining.is_zero() {
                    imp.complete_data_scan(&plan);
                } else {
                    let prefs_weak = prefs.downgrade();
                    // GLib timeout callbacks run on the GTK main loop; the
                    // weak dialog reference prevents a delayed fast-scan result
                    // from keeping a closed Preferences dialog alive.
                    glib::timeout_add_local_once(remaining, move || {
                        if let Some(prefs) = prefs_weak.upgrade() {
                            prefs.imp().complete_data_scan(&plan);
                        }
                    });
                }
            },
        );
    }

    /// Run a supported conversion from the Data page.
    fn run_data_convert(&self) {
        if self.data_operation_inflight.replace(true) {
            return;
        }
        if !self.data_last_scan_offers_convert.get() {
            self.data_operation_inflight.set(false);
            return;
        }

        self.show_data_operation_in_progress("Updating supported older app data");

        let prefs = self.obj().clone();
        spawn_blocking_then_weak(
            &prefs,
            move || {
                let data_dir = json_store::data_dir();
                let inventory = format_upgrade::scan(&data_dir);
                let plan = format_upgrade::build_plan(&inventory);
                if !plan.offers_convert() {
                    return DataConvertWorkerResult::NoConvert(plan);
                }
                let result =
                    format_upgrade::apply_plan(&data_dir, &plan).map_err(|error| error.to_string());
                DataConvertWorkerResult::Applied { plan, result }
            },
            |prefs, result| {
                let imp = prefs.imp();
                imp.data_operation_inflight.set(false);
                imp.data_scan_button.set_sensitive(true);
                match result {
                    DataConvertWorkerResult::NoConvert(plan) => {
                        let detail = if plan.has_no_action() {
                            None
                        } else {
                            Some("No supported conversion is available for the current scan")
                        };
                        imp.render_data_plan(&plan, detail);
                    }
                    DataConvertWorkerResult::Applied {
                        result: Ok(outcome),
                        ..
                    } if outcome.is_success() => imp.run_data_scan(DataScanPresentation::Immediate),
                    DataConvertWorkerResult::Applied {
                        plan,
                        result: Ok(outcome),
                    } => {
                        let detail = outcome.failures.first().map(|failure| {
                            format!("{}: {}", failure.path.display(), failure.detail)
                        });
                        imp.render_data_plan(&plan, detail.as_deref());
                    }
                    DataConvertWorkerResult::Applied {
                        plan,
                        result: Err(detail),
                    } => {
                        imp.render_data_plan(&plan, Some(&detail));
                    }
                }
            },
        );
    }

    /// Render one scanned format plan into Data page status, details, and action state.
    fn render_data_plan(&self, plan: &format_upgrade::FormatPlan, failure: Option<&str>) {
        let status = data_plan_status(plan, failure);
        let offers_convert = plan.offers_convert();
        let verified_current = failure.is_none() && plan.has_no_action();
        self.data_status_row.set_subtitle(&status);
        self.data_current_indicator.set_visible(verified_current);
        self.data_actions_group.set_visible(offers_convert);
        self.data_convert_row.set_visible(offers_convert);
        self.data_convert_button.set_sensitive(offers_convert);
        self.data_convert_button.set_label(if failure.is_some() {
            "Retry"
        } else {
            "Convert"
        });
        self.data_last_scan_offers_convert.set(offers_convert);
        self.render_data_details(plan, failure);
        self.refresh_data_accessibility_state();
        self.announce_data_plan_status(&status, failure);
    }

    /// Present an active Data-page operation before its background result lands.
    fn show_data_operation_in_progress(&self, subtitle: &str) {
        self.data_current_indicator.set_visible(false);
        self.data_scan_button.set_sensitive(false);
        self.data_convert_button.set_sensitive(false);
        self.data_status_row.set_subtitle(subtitle);
        self.refresh_data_accessibility_state();
    }

    /// Accept a completed scan after any visible verification dwell has elapsed.
    fn complete_data_scan(&self, plan: &format_upgrade::FormatPlan) {
        self.data_operation_inflight.set(false);
        self.data_scan_button.set_sensitive(true);
        self.render_data_plan(plan, None);
    }

    /// Announce the compact Data-page result row after a scan or apply attempt.
    fn announce_data_plan_status(&self, status: &str, failure: Option<&str>) {
        let lane = if failure.is_some() {
            accessibility::AnnouncementLane::Alert
        } else {
            accessibility::AnnouncementLane::StatusUpdate
        };
        let key = if failure.is_some() {
            "app-data-format-failed"
        } else {
            "app-data-format-scan"
        };
        self.data_announcement_throttler.announce_if_allowed(
            &*self.data_status_row,
            lane,
            key,
            &format!("App data format scan complete: {status}"),
        );
    }

    /// Rebuild the bounded per-file detail list for the current Data page plan.
    fn render_data_details(&self, plan: &format_upgrade::FormatPlan, failure: Option<&str>) {
        while let Some(child) = self.data_details_list.first_child() {
            self.data_details_list.remove(&child);
        }

        if let Some(detail) = failure {
            let row = libadwaita::ActionRow::builder()
                .title("Last Attempt Failed")
                .subtitle(detail)
                .build();
            accessibility::apply_row_accessibility(
                &row,
                accessibility::RowAccessibility::new("Last app data update attempt failed")
                    .description(detail),
            );
            self.data_details_list.append(&row);
        }

        if plan.has_no_action() {
            let row = libadwaita::ActionRow::builder()
                .title("Current")
                .subtitle("No app data files require a format update")
                .build();
            accessibility::apply_row_accessibility(
                &row,
                accessibility::RowAccessibility::new("App data format current")
                    .description("No app data files require a format update"),
            );
            self.data_details_list.append(&row);
            return;
        }

        let mut position = 1i32;
        let total_rows = data_details_row_count(plan, failure);
        for group in &plan.groups {
            for planned in group.actions.iter().take(DATA_DETAILS_MAX_ROWS_PER_GROUP) {
                let title = format!(
                    "{}: {}",
                    group.metadata_kind.label(),
                    planned.item.path.display()
                );
                let subtitle = action_summary(planned);
                let row = libadwaita::ActionRow::builder()
                    .title(&title)
                    .subtitle(&subtitle)
                    .build();
                accessibility::apply_row_accessibility(
                    &row,
                    accessibility::RowAccessibility::new(&title)
                        .description(&subtitle)
                        .position(position, total_rows),
                );
                self.data_details_list.append(&row);
                position += 1;
            }
            let omitted = group
                .actions
                .len()
                .saturating_sub(DATA_DETAILS_MAX_ROWS_PER_GROUP);
            if omitted > 0 {
                let title = format!("{}: {} more item(s)", group.metadata_kind.label(), omitted);
                let subtitle = "Additional matching app data is included in the action";
                let row = libadwaita::ActionRow::builder()
                    .title(&title)
                    .subtitle("Additional matching app data is included in the action")
                    .build();
                accessibility::apply_row_accessibility(
                    &row,
                    accessibility::RowAccessibility::new(&title)
                        .description(subtitle)
                        .position(position, total_rows),
                );
                self.data_details_list.append(&row);
                position += 1;
            }
        }
    }

    /// Refresh dynamic accessibility state for the Preferences Data page.
    fn refresh_data_accessibility_state(&self) {
        let status = self
            .data_status_row
            .subtitle()
            .unwrap_or_else(|| "Checking app data formats".into());
        accessibility::set_value_text(&*self.data_status_row, status.as_str());
        accessibility::set_busy(&*self.data_status_row, self.data_operation_inflight.get());
        accessibility::set_busy(&*self.data_scan_button, self.data_operation_inflight.get());
        accessibility::set_disabled(
            &*self.data_scan_button,
            !self.data_scan_button.is_sensitive(),
        );
        accessibility::set_disabled(
            &*self.data_convert_button,
            !self.data_convert_button.is_sensitive(),
        );
        accessibility::set_hidden(&*self.data_convert_row, !self.data_convert_row.is_visible());
        accessibility::set_hidden(
            &*self.data_current_indicator,
            !self.data_current_indicator.is_visible(),
        );
        accessibility::set_value_text(&*self.transparency_button, &self.transparency_label.text());
    }

    /// Keep the workspace width preference aligned with the three named shell presets
    /// instead of exposing the raw GSettings backing value to users.
    fn setup_workspace_sidebar_width_row(&self) {
        let model = gtk4::StringList::new(&[]);
        for preset in WorkspaceSidebarWidthPreset::ALL {
            model.append(preset.label());
        }

        self.workspace_sidebar_width_row.set_model(Some(&model));

        let current = WorkspaceSidebarWidthPreset::from_fraction(
            self.settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION),
        );
        self.workspace_sidebar_width_row
            .set_selected(current.index());

        let settings = self.settings.clone();
        self.workspace_sidebar_width_row
            .connect_selected_notify(move |row| {
                let Some(preset) = WorkspaceSidebarWidthPreset::from_index(row.selected()) else {
                    return;
                };
                if (settings.double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION) - preset.fraction())
                    .abs()
                    > f64::EPSILON
                {
                    let _ = settings
                        .set_double(keys::WORKSPACE_SIDEBAR_WIDTH_FRACTION, preset.fraction());
                }
            });
    }

    fn setup_color_scheme_row(&self) {
        let scheme_manager = sourceview5::StyleSchemeManager::default();
        let model = gtk4::StringList::new(&[]);

        // Collect base scheme IDs only; dark variants (e.g., "Adwaita-dark")
        // are selected automatically based on StyleManager::is_dark().
        let scheme_ids: Vec<String> = scheme_manager
            .scheme_ids()
            .iter()
            .filter(|id| !id.ends_with("-dark"))
            .map(std::string::ToString::to_string)
            .collect();

        for id in &scheme_ids {
            if let Some(scheme) = scheme_manager.scheme(id) {
                model.append(&scheme.name());
            }
        }

        self.style_scheme_row.set_model(Some(&model));

        let current = self.settings.string(keys::STYLE_SCHEME);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "The style-scheme dropdown cannot approach u32::MAX entries in practice"
        )]
        let selected_pos = scheme_ids
            .iter()
            .position(|id| id == current.as_str())
            .unwrap_or(0) as u32;
        self.style_scheme_row.set_selected(selected_pos);

        let settings = self.settings.clone();
        self.style_scheme_row.connect_selected_notify(move |row| {
            let pos = row.selected() as usize;
            if pos < scheme_ids.len() {
                let _ = settings.set_string(keys::STYLE_SCHEME, &scheme_ids[pos]);
            }
        });
    }

    fn setup_font_button(&self) {
        self.font_button
            .set_dialog(&gtk4::FontDialog::builder().build());

        let current = self.settings.string(keys::CUSTOM_FONT);
        let desc = pango::FontDescription::from_string(&current);
        self.font_button.set_font_desc(&desc);

        let settings = self.settings.clone();
        self.font_button.connect_font_desc_notify(move |btn| {
            if let Some(desc) = btn.font_desc() {
                let _ = settings.set_string(keys::CUSTOM_FONT, &desc.to_string());
            }
        });
    }

    /// Mirror the Fedora-style transparency control with a percentage label
    /// while keeping the slider value persisted through GSettings.
    fn setup_transparency_row(&self) {
        // This is one-way UI projection: the adjustment value formats into the
        // label text, and `sync_create()` replaces the old explicit initial
        // label update.
        self.transparency_adjustment
            .bind_property("value", &*self.transparency_label, "label")
            .transform_to(|_: &glib::Binding, value: &glib::Value| {
                let opacity = value.get::<f64>().ok()?;
                Some(transparency_label_text(opacity).to_value())
            })
            .sync_create()
            .build();
        let prefs_weak = self.obj().downgrade();
        self.transparency_adjustment
            .connect_value_changed(move |_| {
                if let Some(prefs) = prefs_weak.upgrade() {
                    prefs.imp().refresh_data_accessibility_state();
                }
            });
    }
}

impl WidgetImpl for LushtextPreferences {}
impl AdwDialogImpl for LushtextPreferences {}
impl PreferencesDialogImpl for LushtextPreferences {}

/// Format one stored opacity value as a whole-percent label for the row suffix.
fn transparency_label_text(opacity: f64) -> String {
    format!("{:>3.0}%", (opacity.clamp(0.0, 1.0) * 100.0).floor())
}

/// Summarize a scan/apply result in the compact Data page status row.
fn data_plan_status(plan: &format_upgrade::FormatPlan, failure: Option<&str>) -> String {
    if let Some(detail) = failure {
        return format!("Data update failed: {detail}");
    }
    if plan.has_no_action() {
        "Data format is current".to_string()
    } else if plan.has_future_version_blocker() {
        "Some app data was created by a newer LushText".to_string()
    } else if plan.offers_convert() {
        "Supported older app data can be updated".to_string()
    } else {
        "Some app data needs preservation or recovery".to_string()
    }
}

/// Count the rows rendered into the Data-page details list for position metadata.
fn data_details_row_count(plan: &format_upgrade::FormatPlan, failure: Option<&str>) -> i32 {
    let failure_rows = i32::from(failure.is_some());
    if plan.has_no_action() {
        return failure_rows + 1;
    }

    let planned_rows = plan.groups.iter().fold(0usize, |count, group| {
        let visible = group.actions.len().min(DATA_DETAILS_MAX_ROWS_PER_GROUP);
        let omitted = usize::from(group.actions.len() > DATA_DETAILS_MAX_ROWS_PER_GROUP);
        count + visible + omitted
    });
    i32::try_from(planned_rows)
        .unwrap_or(i32::MAX - failure_rows)
        .saturating_add(failure_rows)
}

/// Convert one planned item into the short subtitle shown in the details list.
fn action_summary(planned: &format_upgrade::FormatPlannedItem) -> String {
    match &planned.action {
        format_upgrade::FormatPlanAction::NoAction => "No action required".to_string(),
        format_upgrade::FormatPlanAction::ConvertToLatest {
            from_version,
            to_version,
        } => format!("Convert from v{from_version} to v{to_version}"),
        format_upgrade::FormatPlanAction::StartFreshOnly => {
            "No converter is available; Start Fresh preserves this data first".to_string()
        }
        format_upgrade::FormatPlanAction::ReportOnly => {
            "Recovery metadata will preserve or report this issue".to_string()
        }
    }
}
