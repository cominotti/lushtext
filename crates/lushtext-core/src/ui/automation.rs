// SPDX-License-Identifier: GPL-3.0-or-later

//! App-owned D-Bus automation adapter.
//!
//! Mutating workflows stay on normal GTK/GIO actions. This module registers a
//! narrow read-only object for action catalog discovery and bounded state
//! snapshots, plus readiness waits that poll app-owned GTK state without
//! driving private widget mutations.

use crate::app::LushtextApplication;
use crate::config;
use crate::model::automation::{
    AUTOMATION_WORKFLOW_CONTENT_SEARCH, AUTOMATION_WORKFLOW_FILE_LOAD,
    AUTOMATION_WORKFLOW_MINIMAP_REFRESH, AUTOMATION_WORKFLOW_REPLACE_PREVIEW,
    AUTOMATION_WORKFLOW_SAVE, AUTOMATION_WORKFLOW_SEARCH, AUTOMATION_WORKFLOW_SESSION_RESTORE,
    AUTOMATION_WORKFLOW_WORKSPACE_REFRESH, AutomationCommandPaletteSnapshot,
    AutomationContentSearchSnapshot, AutomationLocalHistorySnapshot,
    AutomationNativeMinimapDiagnosticSnapshot, AutomationNotesSnapshot,
    AutomationNotificationSnapshot, AutomationReadinessPredicate, AutomationReadinessResult,
    AutomationReadinessStatus, AutomationSearchSnapshot, AutomationSnapshot,
    AutomationSurfaceSnapshot, AutomationTabSnapshot, AutomationVisualAdjustmentSnapshot,
    AutomationVisualGeometrySnapshot, AutomationVisualPixelAnchorSnapshot, AutomationVisualRect,
    AutomationVisualScrollAnchorSnapshot, AutomationVisualSize, AutomationVisualSurfaceSnapshot,
    AutomationWindowSnapshot, AutomationWorkflowEventsSnapshot, AutomationWorkflowObservation,
    AutomationWorkspaceSnapshot, READINESS_BLOCKER_APP_STARTUP, READINESS_BLOCKER_CLOSE_SAFETY,
    READINESS_BLOCKER_COMMAND_PALETTE_INDEX, READINESS_BLOCKER_COMMAND_PALETTE_SEARCH,
    READINESS_BLOCKER_DRAFT_AUTOSAVE, READINESS_BLOCKER_EDITOR_SEARCH, READINESS_BLOCKER_FILE_LOAD,
    READINESS_BLOCKER_MINIMAP_REFRESH, READINESS_BLOCKER_PREVIEW_ANIMATION,
    READINESS_BLOCKER_REPLACE_PREVIEW, READINESS_BLOCKER_SAVE, READINESS_BLOCKER_SESSION_RESTORE,
    READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION, READINESS_BLOCKER_WORKSPACE_PERSIST,
    READINESS_BLOCKER_WORKSPACE_SEARCH, READINESS_BLOCKER_WORKSPACE_SIDEBAR_ANIMATION,
    READINESS_BLOCKER_WORKSPACE_TREE_REFRESH,
};
use crate::model::palette::SearchMode;
use crate::model::workspace::WorkspaceScope;
use crate::services::action_catalog;
use crate::services::local_history_service::LocalHistoryAvailability;
use crate::services::notifications::NotificationSeverity;
use crate::ui::editor_page::{
    EditorLoadState, LushtextEditorPage, MinimapAdjustmentDiagnostics,
    MinimapNativeSliderDiagnostics, MinimapProjectedBounds, MinimapTextViewRect,
};
use crate::ui::window::LushtextWindow;
use gio::prelude::*;
use glib::prelude::ToVariant;
use glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use std::time::{Duration, Instant};

/// Stable interface version for the first automation contract.
pub const INTERFACE_VERSION: u32 = 2;
/// D-Bus interface name exposed by the app-owned automation object.
pub const INTERFACE_NAME: &str = "dev.cominotti.lushtext.Automation1";
/// Child object path segment appended to the normal application object path.
pub const OBJECT_SEGMENT: &str = "Automation";

/// Maximum UTF-8 bytes copied for any free-form text field in one snapshot.
///
/// Four KiB keeps D-Bus snapshot JSON comfortably small for polling agents
/// while preserving enough context to debug ordinary queries, paths, and
/// status-bar messages.
const SNAPSHOT_TEXT_MAX_BYTES: usize = 4 * 1024;
/// Suffix appended when a snapshot text field is shortened.
///
/// The marker is ASCII so truncation stays easy to inspect in JSON logs and
/// does not depend on terminal/font Unicode support.
const SNAPSHOT_TRUNCATION_MARKER: &str = " [truncated]";

/// Stable D-Bus error name used when the app weak reference is already gone.
const ERROR_UNAVAILABLE: &str = "dev.cominotti.lushtext.Automation1.Error.Unavailable";
/// Stable D-Bus error name for serialization or catalog construction failures.
const ERROR_INTERNAL: &str = "dev.cominotti.lushtext.Automation1.Error.Internal";
/// Stable D-Bus error name for callers using an unsupported method name.
const ERROR_UNKNOWN_METHOD: &str = "dev.cominotti.lushtext.Automation1.Error.UnknownMethod";
/// Maximum time one readiness request may keep the GTK main context polling.
const WAIT_FOR_READY_MAX_TIMEOUT: Duration = Duration::from_secs(30);
/// Fast polling window used while ordinary GTK callbacks and animations settle.
const WAIT_FOR_READY_FAST_WINDOW: Duration = Duration::from_secs(1);
/// Initial readiness poll interval, roughly one 60 Hz frame.
const WAIT_FOR_READY_FAST_POLL: Duration = Duration::from_millis(16);
/// Backoff interval for long waits so leaked clients do not wake GTK at 60 Hz.
const WAIT_FOR_READY_SLOW_POLL: Duration = Duration::from_millis(100);
/// Diagnostic surface name for GTK's native source-map viewport slider.
///
/// The name intentionally says `native` instead of `overlay` so visual smoke
/// artifacts do not suggest that LushText owns a replacement highlight.
const MINIMAP_NATIVE_VIEWPORT_SURFACE: &str = "minimap-native-viewport";

/// Static D-Bus introspection contract exported beside the dispatcher.
///
/// Keeping the XML near the method/property handlers makes review drift
/// obvious, and `make check-automation-docs` turns any new member into a
/// documentation requirement.
const INTROSPECTION_XML: &str = r#"
<node>
  <interface name='dev.cominotti.lushtext.Automation1'>
    <property name='InterfaceVersion' type='u' access='read'/>
    <property name='Enabled' type='b' access='read'/>
    <property name='BuildProfile' type='s' access='read'/>
    <method name='GetActionCatalog'>
      <arg type='s' name='json' direction='out'/>
    </method>
    <method name='GetSnapshot'>
      <arg type='s' name='json' direction='out'/>
    </method>
    <method name='GetReadinessPredicates'>
      <arg type='s' name='json' direction='out'/>
    </method>
    <method name='GetWorkflowEvents'>
      <arg type='s' name='json' direction='out'/>
    </method>
    <method name='WaitForReady'>
      <arg type='s' name='predicate' direction='in'/>
      <arg type='u' name='timeout_msec' direction='in'/>
      <arg type='b' name='ok' direction='out'/>
      <arg type='s' name='status' direction='out'/>
      <arg type='s' name='detail' direction='out'/>
    </method>
    <method name='WaitForIdle'>
      <arg type='u' name='timeout_msec' direction='in'/>
      <arg type='b' name='ok' direction='out'/>
      <arg type='s' name='detail' direction='out'/>
    </method>
  </interface>
</node>
"#;

/// Registered automation object lifetime handle.
pub struct AutomationRegistration {
    /// Session-bus connection that owns the registered object.
    ///
    /// The handle is retained so explicit unregister and Drop can unregister on
    /// the same connection that exported the object.
    connection: gio::DBusConnection,
    /// GIO registration token; set to `None` after explicit unregister.
    ///
    /// Clearing the token prevents Drop from trying to unregister the same
    /// object twice during application shutdown.
    registration_id: Option<gio::RegistrationId>,
    /// Object path where the automation interface is exported.
    pub object_path: String,
}

impl AutomationRegistration {
    /// Unregister the automation object before the application bus name is released.
    pub fn unregister(mut self) {
        if let Some(registration_id) = self.registration_id.take()
            && let Err(error) = self.connection.unregister_object(registration_id)
        {
            tracing::warn!("failed to unregister automation D-Bus object: {error}");
        }
    }
}

impl Drop for AutomationRegistration {
    fn drop(&mut self) {
        if let Some(registration_id) = self.registration_id.take()
            && let Err(error) = self.connection.unregister_object(registration_id)
        {
            tracing::warn!("failed to unregister automation D-Bus object: {error}");
        }
    }
}

/// Return the automation object path for an application D-Bus object path.
#[must_use]
pub fn object_path_for(app_object_path: &str) -> String {
    format!("{}/{OBJECT_SEGMENT}", app_object_path.trim_end_matches('/'))
}

/// Register the app-owned read-only automation object.
///
/// # Errors
///
/// Returns a GLib error if the static introspection XML cannot be parsed or if
/// the object path cannot be registered on the supplied D-Bus connection.
pub fn register(
    app: &LushtextApplication,
    connection: &gio::DBusConnection,
    app_object_path: &str,
) -> Result<AutomationRegistration, glib::Error> {
    let node = gio::DBusNodeInfo::for_xml(INTROSPECTION_XML)?;
    let interface_info = node.lookup_interface(INTERFACE_NAME).ok_or_else(|| {
        glib::Error::new(
            gio::IOErrorEnum::Failed,
            "automation introspection XML is missing the declared interface",
        )
    })?;
    let object_path = object_path_for(app_object_path);
    // Keep only a weak app reference so the exported D-Bus object cannot keep
    // the application alive during shutdown; each call upgrades for one request.
    let app_weak = app.downgrade();

    // This builder installs GIO's D-Bus vtable: one closure handles method
    // calls and one handles property reads. GIO invokes them on this
    // connection's main context, so they must answer quickly or return a local
    // future.
    let registration_id = connection
        .register_object(&object_path, &interface_info)
        .method_call(
            move |_connection,
                  _sender,
                  _object_path,
                  _interface_name,
                  method_name,
                  parameters,
                  invocation| {
                handle_method_call(&app_weak, method_name, &parameters, invocation);
            },
        )
        .property(
            |_connection, _sender, _object_path, _interface_name, property_name| {
                property_value(property_name)
            },
        )
        .build()?;

    Ok(AutomationRegistration {
        connection: connection.clone(),
        registration_id: Some(registration_id),
        object_path,
    })
}

/// Dispatch one Automation1 D-Bus method on the GTK main context.
///
/// Read-only methods answer immediately; wait methods return a local future so
/// callers can poll app-owned state without blocking the main loop.
fn handle_method_call(
    app_weak: &glib::WeakRef<LushtextApplication>,
    method_name: &str,
    parameters: &glib::Variant,
    invocation: gio::DBusMethodInvocation,
) {
    match method_name {
        "GetActionCatalog" => match action_catalog::developer_reference_json() {
            Ok(json) => invocation.return_value(Some(&(json,).to_variant())),
            Err(error) => invocation.return_dbus_error(ERROR_INTERNAL, &error.to_string()),
        },
        "GetSnapshot" => {
            let Some(app) = app_weak.upgrade() else {
                invocation.return_dbus_error(ERROR_UNAVAILABLE, "application is no longer alive");
                return;
            };
            // Snapshot reads are read-only toward user data and GTK widgets, but
            // they do refresh the internal diagnostic workflow-event log.
            match serde_json::to_string_pretty(&app_snapshot(&app)) {
                Ok(json) => invocation.return_value(Some(&(json,).to_variant())),
                Err(error) => invocation.return_dbus_error(ERROR_INTERNAL, &error.to_string()),
            }
        }
        "GetReadinessPredicates" => {
            match serde_json::to_string_pretty(&AutomationReadinessPredicate::reference_rows()) {
                Ok(json) => invocation.return_value(Some(&(json,).to_variant())),
                Err(error) => invocation.return_dbus_error(ERROR_INTERNAL, &error.to_string()),
            }
        }
        "GetWorkflowEvents" => {
            let Some(app) = app_weak.upgrade() else {
                invocation.return_dbus_error(ERROR_UNAVAILABLE, "application is no longer alive");
                return;
            };
            match serde_json::to_string_pretty(&refresh_workflow_events(&app)) {
                Ok(json) => invocation.return_value(Some(&(json,).to_variant())),
                Err(error) => invocation.return_dbus_error(ERROR_INTERNAL, &error.to_string()),
            }
        }
        "WaitForReady" => {
            let Some(app) = app_weak.upgrade() else {
                invocation.return_dbus_error(ERROR_UNAVAILABLE, "application is no longer alive");
                return;
            };
            let predicate = parameters.child_get::<String>(0);
            let timeout_msec = parameters.child_get::<u32>(1);
            // WaitForReady is the structured wait API: it replies with
            // ok/status/detail so host helpers can distinguish timeouts from
            // workflow failures and unsupported predicates.
            // The reply future stays on the GLib main context. It may await
            // timers, but all GTK state reads still happen on the main thread.
            invocation.return_future_local(async move {
                let result = wait_for_ready(app, predicate, timeout_msec).await;
                Ok(Some(
                    (result.ok, result.status.to_string(), result.detail).to_variant(),
                ))
            });
        }
        "WaitForIdle" => {
            let Some(app) = app_weak.upgrade() else {
                invocation.return_dbus_error(ERROR_UNAVAILABLE, "application is no longer alive");
                return;
            };
            let timeout_msec = parameters.child_get::<u32>(0);
            // WaitForIdle keeps the legacy two-field reply shape for older
            // smoke helpers; new helpers should prefer WaitForReady("idle").
            // The reply future stays on the GLib main context. It may await
            // timers, but all GTK state reads still happen on the main thread.
            invocation.return_future_local(async move {
                let (ok, detail) = wait_for_idle(app, timeout_msec).await;
                Ok(Some((ok, detail).to_variant()))
            });
        }
        _ => invocation.return_dbus_error(ERROR_UNKNOWN_METHOD, "unknown automation method"),
    }
}

fn property_value(property_name: &str) -> glib::Variant {
    match property_name {
        "InterfaceVersion" => INTERFACE_VERSION.to_variant(),
        "Enabled" => true.to_variant(),
        "BuildProfile" => build_profile().to_variant(),
        _ => String::new().to_variant(),
    }
}

/// Collect a bounded app snapshot from already-mounted GTK state on the main context.
///
/// GTK widgets are main-thread-only, and this projection must stay read-only:
/// expose visible metadata, counts, and flags, never document bodies, sidecars,
/// draft contents, local-history files, or Replace All backup contents. It may
/// advance the internal workflow-event log because that log is diagnostic state,
/// not a user-visible mutation or persistence change.
#[must_use]
pub fn app_snapshot(app: &LushtextApplication) -> AutomationSnapshot {
    refresh_workflow_events(app);
    let idle_blocker = current_idle_blocker(app);
    // GTK exposes the active window through the generic ApplicationWindow
    // type; downcast asks GLib's runtime type system for LushText's concrete
    // window so the adapter can read app-specific state.
    let window = app
        .active_window()
        .and_then(|window| window.downcast::<LushtextWindow>().ok())
        .map(|window| window_snapshot(&window));

    AutomationSnapshot {
        interface_version: INTERFACE_VERSION,
        enabled: true,
        app_id: app
            .application_id()
            .map_or_else(String::new, |id| id.to_string()),
        app_version: config::VERSION.to_string(),
        build_profile: build_profile().to_string(),
        idle: idle_blocker.is_none(),
        idle_blocker,
        window,
    }
}

/// Return the first app-owned workflow that prevents the app from being idle.
#[must_use]
pub fn current_idle_blocker(app: &LushtextApplication) -> Option<String> {
    current_readiness_blocker(app, AutomationReadinessPredicate::Idle)
}

/// Wait for the broad idle predicate and return the legacy WaitForIdle response shape.
async fn wait_for_idle(app: LushtextApplication, timeout_msec: u32) -> (bool, String) {
    let result =
        wait_for_ready_for_predicate(app, AutomationReadinessPredicate::Idle, timeout_msec).await;
    if result.ok {
        return (true, "idle".to_string());
    }
    (
        false,
        result.blocker.unwrap_or_else(|| result.detail.clone()),
    )
}

/// Parse a caller-supplied predicate name before polling live GTK readiness state.
async fn wait_for_ready(
    app: LushtextApplication,
    predicate_name: String,
    timeout_msec: u32,
) -> AutomationReadinessResult {
    let Some(predicate) = AutomationReadinessPredicate::from_name(&predicate_name) else {
        let detail = format!(
            "unknown readiness predicate: {}",
            bounded_snapshot_text(&predicate_name)
        );
        return readiness_result(
            predicate_name,
            AutomationReadinessStatus::UnknownPredicate,
            detail,
            None,
        );
    };
    wait_for_ready_for_predicate(app, predicate, timeout_msec).await
}

/// Poll a readiness predicate until it is ready, fails, or times out.
///
/// This runs on the GTK main context because blocker checks read live widgets.
/// The timeout is clamped and polling backs off so D-Bus callers cannot keep
/// the app spinning at frame rate forever.
async fn wait_for_ready_for_predicate(
    app: LushtextApplication,
    predicate: AutomationReadinessPredicate,
    timeout_msec: u32,
) -> AutomationReadinessResult {
    // Clamp caller-provided timeouts; the wait is cooperative, but still uses
    // main-context timers and should not be held open indefinitely.
    let timeout = Duration::from_millis(u64::from(timeout_msec)).min(WAIT_FOR_READY_MAX_TIMEOUT);
    let start = Instant::now();
    let deadline = start + timeout;

    loop {
        refresh_workflow_events(&app);
        let elapsed = start.elapsed();
        let poll_interval = if elapsed < WAIT_FOR_READY_FAST_WINDOW {
            WAIT_FOR_READY_FAST_POLL
        } else {
            WAIT_FOR_READY_SLOW_POLL
        };
        if let Some(detail) = current_readiness_failure(&app, predicate) {
            return readiness_result(
                predicate.as_str(),
                AutomationReadinessStatus::WorkflowFailure,
                detail,
                None,
            );
        }

        let blocker = current_readiness_blocker(&app, predicate);
        if blocker.is_none() {
            // Yield one settle pass so callbacks already queued behind the
            // state transition can run before automation asserts the snapshot.
            // timeout_future is a GLib main-context timer: awaiting it yields
            // to GTK's main loop and resumes this local future on the main
            // thread, so widget reads after the await remain safe.
            glib::timeout_future(poll_interval).await;
            if current_readiness_failure(&app, predicate).is_none()
                && current_readiness_blocker(&app, predicate).is_none()
            {
                return readiness_result(
                    predicate.as_str(),
                    AutomationReadinessStatus::Ready,
                    format!("{} is ready", predicate.as_str()),
                    None,
                );
            }
        }

        if Instant::now() >= deadline {
            let detail = blocker.as_ref().map_or_else(
                || format!("timed out before {} settled", predicate.as_str()),
                |blocker| {
                    format!(
                        "timed out waiting for {}: blocked by {blocker}",
                        predicate.as_str()
                    )
                },
            );
            return readiness_result(
                predicate.as_str(),
                AutomationReadinessStatus::PredicateTimeout,
                detail,
                blocker,
            );
        }
        glib::timeout_future(poll_interval).await;
    }
}

/// Normalize readiness outcomes so D-Bus replies and tests share one status mapping.
fn readiness_result(
    predicate: impl Into<String>,
    status: AutomationReadinessStatus,
    detail: String,
    blocker: Option<String>,
) -> AutomationReadinessResult {
    AutomationReadinessResult {
        predicate: predicate.into(),
        ok: status == AutomationReadinessStatus::Ready,
        status: status.as_str(),
        detail,
        blocker,
    }
}

/// Test hook for exercising readiness waits without going through D-Bus.
#[cfg(feature = "test-utils")]
pub async fn wait_for_idle_for_test(app: LushtextApplication, timeout_msec: u32) -> (bool, String) {
    wait_for_idle(app, timeout_msec).await
}

/// Test hook for exercising named readiness waits without going through D-Bus.
#[cfg(feature = "test-utils")]
pub async fn wait_for_ready_for_test(
    app: LushtextApplication,
    predicate: AutomationReadinessPredicate,
    timeout_msec: u32,
) -> AutomationReadinessResult {
    wait_for_ready_for_predicate(app, predicate, timeout_msec).await
}

fn current_readiness_blocker(
    app: &LushtextApplication,
    predicate: AutomationReadinessPredicate,
) -> Option<String> {
    let Some(window) = active_lushtext_window(app) else {
        return predicate
            .includes_blocker(READINESS_BLOCKER_APP_STARTUP)
            .then(|| READINESS_BLOCKER_APP_STARTUP.to_string());
    };
    window_readiness_blocker(&window, predicate).map(ToOwned::to_owned)
}

/// Return terminal workflow failures that should stop a readiness wait early.
///
/// Today this is limited to file-open completion: a failed editor load is
/// settled, but it is not a successful ready state for agents opening a file.
fn current_readiness_failure(
    app: &LushtextApplication,
    predicate: AutomationReadinessPredicate,
) -> Option<String> {
    if predicate != AutomationReadinessPredicate::FileOpenComplete {
        return None;
    }
    let window = active_lushtext_window(app)?;
    for index in 0..window.imp().tab_view.n_pages() {
        let page = window.imp().tab_view.nth_page(index);
        let Ok(editor) = page.child().downcast::<LushtextEditorPage>() else {
            continue;
        };
        if editor.load_state() == EditorLoadState::Failed {
            return Some(
                "file-open-complete failed because an editor tab failed to load".to_string(),
            );
        }
    }
    None
}

/// Refresh app-owned workflow events from the same state used by readiness waits.
///
/// This intentionally records equivalent state-change events instead of wiring
/// every workflow module to an event bus. Automation clients usually call this
/// while waiting for readiness, so start/finish transitions line up with the
/// observable conditions agents already assert.
fn refresh_workflow_events(app: &LushtextApplication) -> AutomationWorkflowEventsSnapshot {
    let observations = active_lushtext_window(app)
        .map_or_else(inactive_workflow_observations, |window| {
            window_workflow_observations(&window)
        });
    app.observe_automation_workflows(observations)
}

/// Invalidate live workflow state when no LushText window is available.
///
/// The event log is pull-derived: omitted workflows keep their previous state.
/// Supplying inactive observations here ensures window teardown or recreation
/// emits finish transitions instead of leaving old workflows logically active.
fn inactive_workflow_observations() -> Vec<AutomationWorkflowObservation> {
    [
        AUTOMATION_WORKFLOW_FILE_LOAD,
        AUTOMATION_WORKFLOW_SAVE,
        AUTOMATION_WORKFLOW_SEARCH,
        AUTOMATION_WORKFLOW_WORKSPACE_REFRESH,
        AUTOMATION_WORKFLOW_CONTENT_SEARCH,
        AUTOMATION_WORKFLOW_REPLACE_PREVIEW,
        AUTOMATION_WORKFLOW_SESSION_RESTORE,
        AUTOMATION_WORKFLOW_MINIMAP_REFRESH,
    ]
    .into_iter()
    .map(|workflow_id| AutomationWorkflowObservation::new(workflow_id, false, None))
    .collect()
}

/// Convert live window flags into the stable workflow IDs documented for D-Bus.
///
/// This folds widget state into poll-derived events rather than consuming a
/// dedicated event bus. Aggregate workflows such as `workspace-refresh` use the
/// readiness blocker as the authoritative explanation for why they are active.
/// `recovery-restore-complete` remains a readiness predicate only until a
/// dedicated recovery-specific live flag exists.
fn window_workflow_observations(window: &LushtextWindow) -> Vec<AutomationWorkflowObservation> {
    // `imp()` exposes LushtextWindow's private GObject instance data. This
    // adapter runs on the GTK main context, so it may read widget-owned fields
    // directly while keeping the D-Bus projection read-only.
    let imp = window.imp();
    let mut file_load_active = false;
    let mut save_active = false;
    let mut editor_search_active = false;
    let mut minimap_refresh_active = false;

    for index in 0..imp.tab_view.n_pages() {
        let page = imp.tab_view.nth_page(index);
        let Ok(editor) = page.child().downcast::<LushtextEditorPage>() else {
            continue;
        };
        file_load_active |= editor.load_state() == EditorLoadState::Loading;
        save_active |= editor.is_saving();
        editor_search_active |= editor
            .search_bar()
            .search_context()
            .is_some_and(|context| context.occurrences_count() < 0);
        minimap_refresh_active |= editor.minimap_refresh_blocks_readiness();
    }

    let workspace_refresh_blocker = window_readiness_blocker(
        window,
        AutomationReadinessPredicate::WorkspaceRefreshComplete,
    );
    vec![
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            file_load_active,
            file_load_active.then_some(READINESS_BLOCKER_FILE_LOAD),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_SAVE,
            save_active,
            save_active.then_some(READINESS_BLOCKER_SAVE),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_SEARCH,
            editor_search_active,
            editor_search_active.then_some(READINESS_BLOCKER_EDITOR_SEARCH),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_WORKSPACE_REFRESH,
            workspace_refresh_blocker.is_some(),
            workspace_refresh_blocker,
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_CONTENT_SEARCH,
            imp.search_panel.is_searching(),
            imp.search_panel
                .is_searching()
                .then_some(READINESS_BLOCKER_WORKSPACE_SEARCH),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_REPLACE_PREVIEW,
            imp.search_panel.replace_preview_pending(),
            imp.search_panel
                .replace_preview_pending()
                .then_some(READINESS_BLOCKER_REPLACE_PREVIEW),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_SESSION_RESTORE,
            imp.session.restoring.get(),
            imp.session
                .restoring
                .get()
                .then_some(READINESS_BLOCKER_SESSION_RESTORE),
        ),
        AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_MINIMAP_REFRESH,
            minimap_refresh_active,
            minimap_refresh_active.then_some(READINESS_BLOCKER_MINIMAP_REFRESH),
        ),
    ]
}

fn active_lushtext_window(app: &LushtextApplication) -> Option<LushtextWindow> {
    // GTK returns a generic ApplicationWindow; downcast asks GLib's runtime
    // type system whether it is our concrete LushtextWindow.
    app.active_window()
        .and_then(|window| window.downcast::<LushtextWindow>().ok())
}

/// Inspect live window state in diagnostic-priority order for one predicate.
///
/// This adapter maps GTK widget/service flags to stable readiness blocker
/// names. First match wins for public timeout diagnostics, and unrelated
/// blockers are skipped instead of hiding later blockers that belong to the
/// requested predicate.
fn window_readiness_blocker(
    window: &LushtextWindow,
    predicate: AutomationReadinessPredicate,
) -> Option<&'static str> {
    let imp = window.imp();
    if let Some(blocker) = included_blocker(
        predicate,
        imp.session.restoring.get(),
        READINESS_BLOCKER_SESSION_RESTORE,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.session.close_safety_inflight.get(),
        READINESS_BLOCKER_CLOSE_SAFETY,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        window.draft_workflow_blocks_readiness(),
        READINESS_BLOCKER_DRAFT_AUTOSAVE,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.preview_transition_settle.pending() || imp.markdown_preview.render_pending(),
        READINESS_BLOCKER_PREVIEW_ANIMATION,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        window.workspace_sidebar_transition_pending(),
        READINESS_BLOCKER_WORKSPACE_SIDEBAR_ANIMATION,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.search_panel.is_searching(),
        READINESS_BLOCKER_WORKSPACE_SEARCH,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.command_palette.is_searching(),
        READINESS_BLOCKER_COMMAND_PALETTE_SEARCH,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.command_palette.pending_index_update_count() > 0
            || imp.file_index_builds.borrow().has_work()
            || imp.command_palette_note_refreshes.borrow().has_work(),
        READINESS_BLOCKER_COMMAND_PALETTE_INDEX,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.search_panel.replace_preview_pending(),
        READINESS_BLOCKER_REPLACE_PREVIEW,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.sidebar.workspace_refresh_blocks_readiness(),
        READINESS_BLOCKER_WORKSPACE_TREE_REFRESH,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.sidebar.workspace_persistence_pending(),
        READINESS_BLOCKER_WORKSPACE_PERSIST,
    ) {
        return Some(blocker);
    }
    if let Some(blocker) = included_blocker(
        predicate,
        imp.sidebar.imp().workspace_filter_animation_active.get(),
        READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION,
    ) {
        return Some(blocker);
    }
    for index in 0..imp.tab_view.n_pages() {
        let page = imp.tab_view.nth_page(index);
        let Ok(editor) = page.child().downcast::<LushtextEditorPage>() else {
            continue;
        };
        if let Some(blocker) = included_blocker(
            predicate,
            editor.load_state() == EditorLoadState::Loading,
            READINESS_BLOCKER_FILE_LOAD,
        ) {
            return Some(blocker);
        }
        if let Some(blocker) =
            included_blocker(predicate, editor.is_saving(), READINESS_BLOCKER_SAVE)
        {
            return Some(blocker);
        }
        if let Some(blocker) = included_blocker(
            predicate,
            editor
                .search_bar()
                .search_context()
                .is_some_and(|context| context.occurrences_count() < 0),
            READINESS_BLOCKER_EDITOR_SEARCH,
        ) {
            return Some(blocker);
        }
        if let Some(blocker) = included_blocker(
            predicate,
            editor.minimap_refresh_blocks_readiness(),
            READINESS_BLOCKER_MINIMAP_REFRESH,
        ) {
            return Some(blocker);
        }
    }

    None
}

fn included_blocker(
    predicate: AutomationReadinessPredicate,
    active: bool,
    blocker: &'static str,
) -> Option<&'static str> {
    (active && predicate.includes_blocker(blocker)).then_some(blocker)
}

/// Build the full active-window projection from mounted widgets only.
///
/// This stays on the GTK main context and delegates to per-area helpers so each
/// snapshot keeps its own privacy boundary.
fn window_snapshot(window: &LushtextWindow) -> AutomationWindowSnapshot {
    let imp = window.imp();
    let selected_page = imp.tab_view.selected_page();
    let tab_count_i32 = imp.tab_view.n_pages();
    let tab_count = u32::try_from(tab_count_i32).unwrap_or_default();
    let mut active_tab_index = None;
    let mut tabs = Vec::with_capacity(usize::try_from(tab_count_i32).unwrap_or_default());

    for index in 0..tab_count_i32 {
        let page = imp.tab_view.nth_page(index);
        let active = selected_page.as_ref() == Some(&page);
        let snapshot_index = u32::try_from(index).unwrap_or_default();
        if active {
            active_tab_index = Some(snapshot_index);
        }
        tabs.push(tab_snapshot(snapshot_index, active, &page));
    }

    AutomationWindowSnapshot {
        tab_count,
        active_tab_index,
        tabs,
        surfaces: surface_snapshot(window),
        search: search_snapshot(window),
        workspace: workspace_snapshot(window),
        command_palette: command_palette_snapshot(window),
        notes: notes_snapshot(window),
        local_history: local_history_snapshot(window),
        content_search: content_search_snapshot(window),
        notifications: notification_snapshot(window),
        visual_geometry: visual_geometry_snapshot(window),
    }
}

/// Project one tab into non-content metadata for automation clients.
///
/// Titles and paths are bounded; buffer text, draft IDs, and sidecar identities
/// are intentionally omitted.
fn tab_snapshot(index: u32, active: bool, page: &libadwaita::TabPage) -> AutomationTabSnapshot {
    let editor = page.child().downcast::<LushtextEditorPage>().ok();
    let path = editor.as_ref().and_then(LushtextEditorPage::file_path);
    AutomationTabSnapshot {
        index,
        active,
        title: editor.as_ref().map_or_else(
            || bounded_snapshot_text(page.title()),
            |editor| bounded_snapshot_text(editor.title()),
        ),
        document_kind: if path.is_some() { "file" } else { "untitled" }.to_string(),
        path: path.map(|path| bounded_snapshot_text(path.display().to_string())),
        modified: editor.as_ref().is_some_and(LushtextEditorPage::is_modified),
        saving: editor.as_ref().is_some_and(LushtextEditorPage::is_saving),
        load_state: editor.as_ref().map_or_else(
            || "unknown".to_string(),
            |editor| load_state_name(editor.load_state()).to_string(),
        ),
        file_size: editor.as_ref().and_then(LushtextEditorPage::file_size),
        draft_present: editor
            .as_ref()
            .and_then(LushtextEditorPage::draft_id)
            .is_some(),
        evicted: editor.as_ref().is_some_and(LushtextEditorPage::is_evicted),
        pinned: page.is_pinned(),
    }
}

/// Summarize workspace scope and persistence state without scanning the filesystem.
fn workspace_snapshot(window: &LushtextWindow) -> AutomationWorkspaceSnapshot {
    let imp = window.imp();
    let workspaces = imp.sidebar.workspaces_file();
    let scope = imp.sidebar.current_scope();
    let scope_workspace_id = scope
        .workspace_id()
        .map(|id| bounded_snapshot_text(id.as_str()));
    let scope_workspace_name = scope.workspace_id().and_then(|id| {
        workspaces
            .workspace(id)
            .map(|workspace| bounded_snapshot_text(&workspace.name))
    });

    AutomationWorkspaceSnapshot {
        scope_kind: workspace_scope_name(&scope).to_string(),
        scope_workspace_id,
        scope_workspace_name,
        workspace_count: bounded_len(workspaces.workspaces.len()),
        folder_count: bounded_len(workspaces.all_workspace_folder_paths().len()),
        scoped_folder_count: bounded_len(imp.sidebar.current_scope_folder_paths().len()),
        no_workspaces: workspaces.workspaces.is_empty(),
        persistence_inflight: imp.sidebar.workspace_persistence_inflight(),
        persistence_dirty: imp.sidebar.workspace_persistence_pending(),
        filter_animation_active: imp.sidebar.imp().workspace_filter_animation_active.get(),
    }
}

/// Summarize command-palette visibility, query, and index counts without result bodies.
fn command_palette_snapshot(window: &LushtextWindow) -> AutomationCommandPaletteSnapshot {
    let imp = window.imp();

    AutomationCommandPaletteSnapshot {
        visible: imp.palette_revealer.reveals_child(),
        searching: imp.command_palette.is_searching(),
        query: bounded_snapshot_text(imp.command_palette.query()),
        mode: search_mode_name(imp.command_palette.mode()).to_string(),
        result_count: imp.command_palette.result_count(),
        file_index_count: bounded_len(imp.command_palette.file_index_len()),
        open_tab_source_count: bounded_len(imp.command_palette.open_tab_source_count()),
        pending_index_update_count: bounded_len(imp.command_palette.pending_index_update_count()),
    }
}

/// Summarize live notes/bookmark availability without loading sidecar files.
fn notes_snapshot(window: &LushtextWindow) -> AutomationNotesSnapshot {
    let imp = window.imp();
    let editor = active_editor(window);
    let active_document_file_backed = editor
        .as_ref()
        .and_then(LushtextEditorPage::file_path)
        .is_some();
    let active_document_bookmark_count = editor
        .as_ref()
        .map_or(0, |editor| bounded_len(editor.bookmark_records().len()));
    let active_line_has_bookmark = editor
        .as_ref()
        .is_some_and(|editor| editor.current_bookmark().is_some());

    AutomationNotesSnapshot {
        notes_menu_open: imp.notes_menu_button.is_active(),
        active_document_file_backed,
        active_document_bookmark_count,
        active_line_has_bookmark,
        document_note_available: active_document_file_backed,
        folder_note_available: !imp.sidebar.current_scope_folder_paths().is_empty(),
    }
}

/// Summarize local-history availability from the active editor policy only.
fn local_history_snapshot(window: &LushtextWindow) -> AutomationLocalHistorySnapshot {
    let editor = active_editor(window);
    let active_document_file_backed = editor
        .as_ref()
        .and_then(LushtextEditorPage::file_path)
        .is_some();
    let availability = editor.as_ref().map_or(
        LocalHistoryAvailability::Unavailable,
        LushtextEditorPage::local_history_availability,
    );

    AutomationLocalHistorySnapshot {
        browse_available: active_document_file_backed && availability.allows_browsing(),
        automatic_capture_available: active_document_file_backed
            && availability.allows_automatic_capture(),
        availability: local_history_availability_name(availability).to_string(),
        active_document_file_backed,
    }
}

/// Summarize workspace search and Replace All state without match bodies or file content.
fn content_search_snapshot(window: &LushtextWindow) -> AutomationContentSearchSnapshot {
    let imp = window.imp();
    let search_panel: &crate::ui::search_panel::LushtextSearchPanel = imp.search_panel.as_ref();

    AutomationContentSearchSnapshot {
        visible: imp.search_panel_revealer.reveals_child(),
        query: bounded_snapshot_text(search_panel.query()),
        regex_enabled: search_panel.regex_enabled(),
        case_sensitive: search_panel.case_sensitive(),
        whole_word_enabled: search_panel.whole_word_enabled(),
        gitignore_enabled: search_panel.gitignore_enabled(),
        glob_filter: search_panel.glob_filter().map(bounded_snapshot_text),
        searching: search_panel.is_searching(),
        file_count: search_panel.total_files(),
        match_count: search_panel.total_matches(),
        result_capped: search_panel.result_capped(),
        replace_query_present: !search_panel.replace_query().is_empty(),
        replace_preview_mode: search_panel.replace_preview_mode(),
        replace_preview_pending: search_panel.replace_preview_pending(),
        replace_preview_count: search_panel.replace_preview_count(),
        checked_replacement_count: search_panel.checked_replacement_count(),
        omitted_replacement_count: search_panel.omitted_replacement_count(),
        skipped_replacement_count: search_panel.skipped_replacement_count(),
        has_undo_backup: search_panel.has_undo_backup(),
        history_count: search_panel.history_count(),
        saved_search_count: search_panel.saved_search_count(),
        navigation_match_count: search_panel.navigation_match_count(),
        current_navigation_match_index: search_panel.current_navigation_match_index(),
    }
}

/// Summarize the visible status/progress notification state.
fn notification_snapshot(window: &LushtextWindow) -> AutomationNotificationSnapshot {
    let imp = window.imp();
    let status = imp.notification_bus.status_bar_view();

    AutomationNotificationSnapshot {
        status_text: status
            .as_ref()
            .map(|message| bounded_snapshot_text(&message.text)),
        status_severity: status
            .map(|message| notification_severity_name(message.severity).to_string()),
        generation: imp.notification_bus.generation(),
        search_progress_visible: imp.search_progress.visible.get(),
    }
}

/// Collect bounded rectangles for surfaces that visual smoke may crop or mask.
///
/// The snapshot uses stable names and window-relative logical pixels only. It
/// never inspects text contents or widget-rendered glyphs, so editor, minimap,
/// preview, notes, and search surfaces stay inside the automation privacy boundary.
fn visual_geometry_snapshot(window: &LushtextWindow) -> AutomationVisualGeometrySnapshot {
    let imp = window.imp();
    let root: &gtk4::Widget = window.upcast_ref();
    let mut surfaces = Vec::new();
    let mut pixel_anchors = Vec::new();
    let native_minimap: AutomationNativeMinimapDiagnosticSnapshot;
    let mut scroll_anchors = Vec::new();

    push_widget_surface(&mut surfaces, "header-bar", &*imp.header_bar, root);
    push_widget_surface(
        &mut surfaces,
        "header-open-menu-button",
        &*imp.open_menu_button,
        root,
    );
    push_widget_surface(
        &mut surfaces,
        "header-new-tab-button",
        &*imp.new_tab_button,
        root,
    );
    push_widget_surface(&mut surfaces, "tab-strip", &*imp.tab_bar, root);
    push_widget_surface(&mut surfaces, "status-bar", &*imp.status_bar, root);
    push_widget_surface(&mut surfaces, "workspace-sidebar", &*imp.sidebar, root);
    push_widget_surface(
        &mut surfaces,
        "document-properties",
        &*imp.properties_panel,
        root,
    );
    push_widget_surface(&mut surfaces, "preview", &*imp.markdown_preview, root);
    push_widget_surface(&mut surfaces, "search-panel", &*imp.search_panel, root);
    let search_imp = imp.search_panel.imp();
    push_widget_surface(
        &mut surfaces,
        "search-results-scroll",
        &*search_imp.results_scroll,
        root,
    );
    push_widget_surface(
        &mut surfaces,
        "search-preview-summary",
        &*search_imp.count_label,
        root,
    );
    push_widget_surface(
        &mut surfaces,
        "search-replace-controls",
        &*search_imp.replace_entry,
        root,
    );
    push_open_popover_surfaces(window, &mut surfaces, root);

    if let Some(editor) = active_editor(window) {
        push_widget_surface(
            &mut surfaces,
            "editor-viewport",
            &*editor.imp().scrolled_window,
            root,
        );
        push_widget_surface(&mut surfaces, "source-view", editor.source_view(), root);
        push_widget_surface(
            &mut surfaces,
            "minimap-shell",
            &*editor.imp().minimap_overlay,
            root,
        );
        if let Some(freeze) = editor
            .imp()
            .minimap
            .render_hold
            .borrow()
            .as_ref()
            .map(|hold| hold.cover().clone())
        {
            push_widget_surface(&mut surfaces, "minimap-reflow-freeze", &freeze, root);
        } else {
            surfaces.push(absent_visual_surface(
                "minimap-reflow-freeze",
                "not-created",
            ));
        }
        if let Some(source_map) = editor.imp().minimap.source_map.borrow().as_ref().cloned() {
            push_widget_surface(&mut surfaces, "minimap-source-map", &source_map, root);
            // These anchors describe GTK's native `GtkSourceMap` slider pixels
            // for screenshot inspection. They must not imply or create an
            // app-owned replacement highlight.
            // Automation snapshots may be polled by external agents. Cache the
            // projected minimap geometry once so pixel anchors do not repeat
            // the same GTK layout-coordinate queries for one snapshot.
            let native_diagnostics = editor.minimap_native_slider_diagnostics_relative_to(root);
            let viewport_bounds = native_diagnostics
                .as_ref()
                .map(|diagnostics| diagnostics.native_slider_visible_bounds);
            let first_content_bounds = native_diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.first_content_row)
                .or_else(|| editor.minimap_first_content_row_relative_to(root));
            native_minimap = native_diagnostics.as_ref().map_or_else(
                || absent_native_minimap_diagnostic(minimap_availability_absence_reason(&editor)),
                native_minimap_diagnostic_snapshot,
            );
            surfaces.push(minimap_viewport_surface(&editor, viewport_bounds));
            pixel_anchors.push(minimap_viewport_top_edge_anchor(&editor, viewport_bounds));
            pixel_anchors.push(minimap_viewport_fill_anchor(&editor, viewport_bounds));
            pixel_anchors.push(minimap_viewport_bottom_edge_anchor(
                &editor,
                viewport_bounds,
            ));
            pixel_anchors.push(minimap_first_content_row_anchor(
                &editor,
                first_content_bounds,
            ));
        } else {
            surfaces.push(absent_visual_surface("minimap-source-map", "not-created"));
            surfaces.push(absent_visual_surface(
                MINIMAP_NATIVE_VIEWPORT_SURFACE,
                "not-created",
            ));
            // Preserve stable absent anchors even when the native source map has
            // not been created, so visual runners can distinguish "skipped by
            // app state" from "missing from the snapshot schema."
            pixel_anchors.push(absent_pixel_anchor(
                "minimap-viewport-top-edge",
                MINIMAP_NATIVE_VIEWPORT_SURFACE,
                "not-created",
            ));
            pixel_anchors.push(absent_pixel_anchor(
                "minimap-viewport-fill",
                MINIMAP_NATIVE_VIEWPORT_SURFACE,
                "not-created",
            ));
            pixel_anchors.push(absent_pixel_anchor(
                "minimap-viewport-bottom-edge",
                MINIMAP_NATIVE_VIEWPORT_SURFACE,
                "not-created",
            ));
            pixel_anchors.push(absent_pixel_anchor(
                "minimap-first-content-row",
                "minimap-source-map",
                "not-created",
            ));
            native_minimap = absent_native_minimap_diagnostic("not-created");
        }
        if let Some(marker_strip) = editor.imp().minimap.marker_strip.borrow().as_ref().cloned() {
            push_widget_surface(&mut surfaces, "minimap-marker-strip", &marker_strip, root);
        } else {
            surfaces.push(absent_visual_surface("minimap-marker-strip", "not-created"));
        }
        if editor.is_search_visible() {
            push_widget_surface(&mut surfaces, "active-transient", editor.search_bar(), root);
        } else {
            push_active_transient_surface(window, &mut surfaces, root);
        }
        let source_view = editor.source_view();
        let hadjustment = source_view.hadjustment();
        let vadjustment = source_view.vadjustment();
        scroll_anchors.push(scroll_anchor_snapshot(
            "source-view",
            hadjustment.as_ref(),
            vadjustment.as_ref(),
            f64::from(source_view.left_margin().max(0)),
            f64::from(source_view.top_margin().max(0)),
        ));
    } else {
        surfaces.push(absent_visual_surface("editor-viewport", "no-active-editor"));
        surfaces.push(absent_visual_surface("source-view", "no-active-editor"));
        surfaces.push(absent_visual_surface("minimap-shell", "no-active-editor"));
        surfaces.push(absent_visual_surface(
            "minimap-source-map",
            "no-active-editor",
        ));
        surfaces.push(absent_visual_surface(
            "minimap-reflow-freeze",
            "no-active-editor",
        ));
        surfaces.push(absent_visual_surface(
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "no-active-editor",
        ));
        surfaces.push(absent_visual_surface(
            "minimap-marker-strip",
            "no-active-editor",
        ));
        // Keep the same anchor names in no-editor snapshots; proof policy uses
        // absence reasons to fail or skip deliberately instead of silently
        // treating omitted anchors as unverified.
        pixel_anchors.push(absent_pixel_anchor(
            "minimap-viewport-top-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "no-active-editor",
        ));
        pixel_anchors.push(absent_pixel_anchor(
            "minimap-viewport-fill",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "no-active-editor",
        ));
        pixel_anchors.push(absent_pixel_anchor(
            "minimap-viewport-bottom-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "no-active-editor",
        ));
        pixel_anchors.push(absent_pixel_anchor(
            "minimap-first-content-row",
            "minimap-source-map",
            "no-active-editor",
        ));
        native_minimap = absent_native_minimap_diagnostic("no-active-editor");
        surfaces.push(absent_visual_surface("active-transient", "none-active"));
    }

    let blocker =
        window_readiness_blocker(window, AutomationReadinessPredicate::VisualGeometrySettled)
            .map(ToOwned::to_owned);

    AutomationVisualGeometrySnapshot {
        scale_factor: window.scale_factor(),
        coordinate_space: "window-logical-pixels".to_string(),
        ready: blocker.is_none(),
        blocker,
        surfaces,
        pixel_anchors,
        native_minimap,
        scroll_anchors,
    }
}

fn push_active_transient_surface(
    window: &LushtextWindow,
    surfaces: &mut Vec<AutomationVisualSurfaceSnapshot>,
    root: &gtk4::Widget,
) {
    let imp = window.imp();
    if imp.palette_revealer.reveals_child() {
        push_widget_surface(surfaces, "active-transient", &*imp.command_palette, root);
    } else if imp.search_panel_revealer.reveals_child() {
        push_widget_surface(surfaces, "active-transient", &*imp.search_panel, root);
    } else if open_popover_visible(window) {
        push_widget_surface(surfaces, "active-transient", &*imp.open_popover, root);
    } else {
        surfaces.push(absent_visual_surface("active-transient", "none-active"));
    }
}

fn push_open_popover_surfaces(
    window: &LushtextWindow,
    surfaces: &mut Vec<AutomationVisualSurfaceSnapshot>,
    root: &gtk4::Widget,
) {
    let imp = window.imp();
    if !open_popover_visible(window) {
        for name in [
            "open-popover",
            "open-popover-search",
            "open-popover-chooser",
            "open-popover-recent-list",
            "open-popover-empty-state",
        ] {
            surfaces.push(absent_visual_surface(name, "popover-closed"));
        }
        return;
    }

    push_widget_surface(surfaces, "open-popover", &*imp.open_popover, root);
    push_widget_surface(
        surfaces,
        "open-popover-search",
        &imp.open_popover.search_entry_widget(),
        root,
    );
    push_widget_surface(
        surfaces,
        "open-popover-chooser",
        &imp.open_popover.chooser_button_widget(),
        root,
    );
    push_widget_surface(
        surfaces,
        "open-popover-recent-list",
        &imp.open_popover.recent_scroller_widget(),
        root,
    );
    push_widget_surface(
        surfaces,
        "open-popover-empty-state",
        &imp.open_popover.empty_state_widget(),
        root,
    );
}

fn open_popover_visible(window: &LushtextWindow) -> bool {
    let imp = window.imp();
    imp.open_menu_button.is_active() || imp.open_popover.is_visible()
}

fn push_widget_surface(
    surfaces: &mut Vec<AutomationVisualSurfaceSnapshot>,
    name: &str,
    widget: &impl IsA<gtk4::Widget>,
    root: &gtk4::Widget,
) {
    surfaces.push(widget_visual_surface(name, widget, root));
}

fn widget_visual_surface(
    name: &str,
    widget: &impl IsA<gtk4::Widget>,
    root: &gtk4::Widget,
) -> AutomationVisualSurfaceSnapshot {
    let widget = widget.as_ref();
    let allocation = positive_allocation(widget);
    let rect = allocation
        .filter(|_| widget.is_visible())
        .and_then(|_| widget.compute_bounds(root))
        .and_then(visual_rect_from_bounds);
    let visible = rect.is_some();
    let absence_reason = (!visible).then(|| widget_absence_reason(widget).to_string());

    AutomationVisualSurfaceSnapshot {
        name: name.to_string(),
        visible,
        rect,
        allocation,
        absence_reason,
    }
}

fn computed_visual_surface(
    name: &str,
    rect: Option<AutomationVisualRect>,
    absence_reason: &'static str,
) -> AutomationVisualSurfaceSnapshot {
    AutomationVisualSurfaceSnapshot {
        name: name.to_string(),
        visible: rect.is_some(),
        allocation: rect.map(|rect| AutomationVisualSize {
            width: rect.width,
            height: rect.height,
        }),
        rect,
        absence_reason: rect.is_none().then(|| absence_reason.to_string()),
    }
}

fn absent_visual_surface(
    name: &str,
    absence_reason: &'static str,
) -> AutomationVisualSurfaceSnapshot {
    computed_visual_surface(name, None, absence_reason)
}

/// Build one stable pixel-anchor record and mirror visibility into absence.
///
/// External visual runners use the rect plus stable absence text to decide
/// whether a screenshot crop is required or intentionally skipped.
fn pixel_anchor(
    name: &str,
    surface: &str,
    rect: Option<AutomationVisualRect>,
    absence_reason: &'static str,
) -> AutomationVisualPixelAnchorSnapshot {
    AutomationVisualPixelAnchorSnapshot {
        name: name.to_string(),
        surface: surface.to_string(),
        visible: rect.is_some(),
        rect,
        absence_reason: rect.is_none().then(|| absence_reason.to_string()),
    }
}

fn absent_pixel_anchor(
    name: &str,
    surface: &str,
    absence_reason: &'static str,
) -> AutomationVisualPixelAnchorSnapshot {
    pixel_anchor(name, surface, None, absence_reason)
}

/// Convert editor-page native minimap diagnostics into the Automation1 shape.
///
/// Raw GTK geometry stays bounded to non-content rectangles while preserving
/// enough data for screenshot artifacts to explain native slider drift.
fn native_minimap_diagnostic_snapshot(
    diagnostics: &MinimapNativeSliderDiagnostics,
) -> AutomationNativeMinimapDiagnosticSnapshot {
    let source_map_rect = visual_rect_from_projected_bounds(diagnostics.source_map_bounds);
    AutomationNativeMinimapDiagnosticSnapshot {
        visible: true,
        absence_reason: None,
        projection_source: Some(diagnostics.projection_source.as_str().to_string()),
        source_map_allocation: source_map_rect.map(|rect| AutomationVisualSize {
            width: rect.width,
            height: rect.height,
        }),
        source_map_rect,
        editor_visible_rect: Some(text_view_rect_snapshot(diagnostics.editor_visible_rect)),
        source_map_visible_rect: Some(text_view_rect_snapshot(diagnostics.source_map_visible_rect)),
        source_view_vadjustment: diagnostics
            .source_view_vadjustment
            .map(adjustment_diagnostic_snapshot),
        source_map_vadjustment: diagnostics
            .source_map_vadjustment
            .map(adjustment_diagnostic_snapshot),
        editor_document_height: Some(diagnostics.editor_document_height),
        source_map_document_height: Some(diagnostics.source_map_document_height),
        border_left: Some(diagnostics.border_left),
        border_right: Some(diagnostics.border_right),
        native_slider_estimate: visual_rect_from_projected_bounds(
            diagnostics.native_slider_estimate,
        ),
        native_slider_visible_bounds: visual_rect_from_projected_bounds(
            diagnostics.native_slider_visible_bounds,
        ),
        line_projection_rect: diagnostics
            .line_projection
            .and_then(visual_rect_from_projected_bounds),
        first_content_row_rect: diagnostics
            .first_content_row
            .and_then(visual_rect_from_projected_bounds),
    }
}

/// Build the full native-minimap shape for skipped or unavailable states.
///
/// Returning every optional field as `None` keeps the schema stable when no
/// active editor, source map, or renderable minimap exists.
fn absent_native_minimap_diagnostic(
    absence_reason: &'static str,
) -> AutomationNativeMinimapDiagnosticSnapshot {
    AutomationNativeMinimapDiagnosticSnapshot {
        visible: false,
        absence_reason: Some(absence_reason.to_string()),
        projection_source: None,
        source_map_allocation: None,
        source_map_rect: None,
        editor_visible_rect: None,
        source_map_visible_rect: None,
        source_view_vadjustment: None,
        source_map_vadjustment: None,
        editor_document_height: None,
        source_map_document_height: None,
        border_left: None,
        border_right: None,
        native_slider_estimate: None,
        native_slider_visible_bounds: None,
        line_projection_rect: None,
        first_content_row_rect: None,
    }
}

fn text_view_rect_snapshot(rect: MinimapTextViewRect) -> AutomationVisualRect {
    AutomationVisualRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn adjustment_diagnostic_snapshot(
    diagnostics: MinimapAdjustmentDiagnostics,
) -> AutomationVisualAdjustmentSnapshot {
    AutomationVisualAdjustmentSnapshot {
        at_lower: diagnostics.at_lower,
        value_milli: diagnostics.value_milli,
        lower_milli: diagnostics.lower_milli,
        upper_milli: diagnostics.upper_milli,
        page_size_milli: diagnostics.page_size_milli,
    }
}

fn positive_allocation(widget: &gtk4::Widget) -> Option<AutomationVisualSize> {
    let width = widget.width();
    let height = widget.height();
    (width > 0 && height > 0).then_some(AutomationVisualSize { width, height })
}

fn widget_absence_reason(widget: &gtk4::Widget) -> &'static str {
    if !widget.property::<bool>("visible") {
        "hidden"
    } else if !widget.is_visible() {
        "ancestor-hidden"
    } else if widget.width() <= 0 || widget.height() <= 0 {
        "zero-allocation"
    } else {
        "bounds-unavailable"
    }
}

/// Expose the native minimap viewport slider bounds as a computed visual surface.
///
/// The surface is read-only diagnostic geometry: it does not create or mutate a
/// widget, and absence reasons tell smoke tools whether the minimap was hidden
/// or whether GTK could not provide stable bounds yet.
fn minimap_viewport_surface(
    editor: &LushtextEditorPage,
    bounds: Option<MinimapProjectedBounds>,
) -> AutomationVisualSurfaceSnapshot {
    if !editor.is_minimap_visible() {
        return absent_visual_surface(
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            minimap_availability_absence_reason(editor),
        );
    }

    let Some(rect) = bounds.and_then(visual_rect_from_projected_bounds) else {
        return absent_visual_surface(MINIMAP_NATIVE_VIEWPORT_SURFACE, "bounds-unavailable");
    };

    computed_visual_surface(
        MINIMAP_NATIVE_VIEWPORT_SURFACE,
        Some(rect),
        "bounds-unavailable",
    )
}

/// Pixel anchor for the top edge of the native minimap viewport slider.
fn minimap_viewport_top_edge_anchor(
    editor: &LushtextEditorPage,
    bounds: Option<MinimapProjectedBounds>,
) -> AutomationVisualPixelAnchorSnapshot {
    if !editor.is_minimap_visible() {
        return absent_pixel_anchor(
            "minimap-viewport-top-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            minimap_availability_absence_reason(editor),
        );
    }

    let Some(rect) = bounds.and_then(|bounds| edge_anchor_rect(bounds.x, bounds.y, bounds.width))
    else {
        return absent_pixel_anchor(
            "minimap-viewport-top-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "bounds-unavailable",
        );
    };

    pixel_anchor(
        "minimap-viewport-top-edge",
        MINIMAP_NATIVE_VIEWPORT_SURFACE,
        Some(rect),
        "bounds-unavailable",
    )
}

/// Pixel anchor for the filled body of the native minimap viewport highlight.
fn minimap_viewport_fill_anchor(
    editor: &LushtextEditorPage,
    bounds: Option<MinimapProjectedBounds>,
) -> AutomationVisualPixelAnchorSnapshot {
    if !editor.is_minimap_visible() {
        return absent_pixel_anchor(
            "minimap-viewport-fill",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            minimap_availability_absence_reason(editor),
        );
    }

    let Some(rect) =
        bounds.and_then(|bounds| fill_anchor_rect(bounds.x, bounds.y, bounds.width, bounds.height))
    else {
        return absent_pixel_anchor(
            "minimap-viewport-fill",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "bounds-unavailable",
        );
    };

    pixel_anchor(
        "minimap-viewport-fill",
        MINIMAP_NATIVE_VIEWPORT_SURFACE,
        Some(rect),
        "bounds-unavailable",
    )
}

/// Pixel anchor for the bottom edge of the native minimap viewport slider.
fn minimap_viewport_bottom_edge_anchor(
    editor: &LushtextEditorPage,
    bounds: Option<MinimapProjectedBounds>,
) -> AutomationVisualPixelAnchorSnapshot {
    if !editor.is_minimap_visible() {
        return absent_pixel_anchor(
            "minimap-viewport-bottom-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            minimap_availability_absence_reason(editor),
        );
    }

    let Some(rect) =
        bounds.and_then(|bounds| bottom_edge_anchor_rect(bounds.x, bounds.bottom(), bounds.width))
    else {
        return absent_pixel_anchor(
            "minimap-viewport-bottom-edge",
            MINIMAP_NATIVE_VIEWPORT_SURFACE,
            "bounds-unavailable",
        );
    };

    pixel_anchor(
        "minimap-viewport-bottom-edge",
        MINIMAP_NATIVE_VIEWPORT_SURFACE,
        Some(rect),
        "bounds-unavailable",
    )
}

/// Pixel anchor for the first rendered minimap text row.
fn minimap_first_content_row_anchor(
    editor: &LushtextEditorPage,
    bounds: Option<MinimapProjectedBounds>,
) -> AutomationVisualPixelAnchorSnapshot {
    if !editor.is_minimap_visible() {
        return absent_pixel_anchor(
            "minimap-first-content-row",
            "minimap-source-map",
            minimap_availability_absence_reason(editor),
        );
    }

    let Some(rect) =
        bounds.and_then(|bounds| content_anchor_rect(bounds.x, bounds.y, bounds.width))
    else {
        return absent_pixel_anchor(
            "minimap-first-content-row",
            "minimap-source-map",
            "bounds-unavailable",
        );
    };

    pixel_anchor(
        "minimap-first-content-row",
        "minimap-source-map",
        Some(rect),
        "bounds-unavailable",
    )
}

/// Return a narrow crop around a rounded horizontal slider edge.
fn edge_anchor_rect(x: f64, y: f64, width: f64) -> Option<AutomationVisualRect> {
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || width <= 0.0 {
        return None;
    }

    let x = gtk_f64_floor_to_i32(x);
    // Include one pixel above and below the rounded edge so fractional GTK
    // allocation and antialiasing still land inside the detector crop.
    let y = gtk_f64_round_to_i32(y).saturating_sub(1);
    let width = gtk_f64_ceil_to_i32(width).max(1);
    Some(AutomationVisualRect {
        x,
        y,
        width,
        height: 3,
    })
}

/// Return a crop that catches the native slider's lower border after GTK rounding.
fn bottom_edge_anchor_rect(x: f64, y: f64, width: f64) -> Option<AutomationVisualRect> {
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || width <= 0.0 {
        return None;
    }

    let x = gtk_f64_floor_to_i32(x);
    // The bottom stroke can sit above the projected bottom after line-height
    // rounding, so use a taller upward crop around the native border.
    let y = gtk_f64_round_to_i32(y).saturating_sub(7);
    let width = gtk_f64_ceil_to_i32(width).max(1);
    Some(AutomationVisualRect {
        x,
        y,
        width,
        height: 10,
    })
}

/// Return a small crop inside the viewport body, away from the crisp edge strokes.
fn fill_anchor_rect(x: f64, y: f64, width: f64, height: f64) -> Option<AutomationVisualRect> {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return None;
    }

    // Only inset when the viewport is tall enough to leave a real body sample;
    // cap the crop so detectors inspect the fill, not unrelated minimap text.
    let inset = if height > 4.0 { 2.0 } else { 0.0 };
    let x = gtk_f64_floor_to_i32(x);
    let y = gtk_f64_round_to_i32(y + inset);
    let width = gtk_f64_ceil_to_i32(width).max(1);
    let height = gtk_f64_ceil_to_i32((height - inset).clamp(1.0, 10.0));
    Some(AutomationVisualRect {
        x,
        y,
        width,
        height,
    })
}

/// Return a crop around the first visible minimap content row.
fn content_anchor_rect(x: f64, y: f64, width: f64) -> Option<AutomationVisualRect> {
    if !x.is_finite() || !y.is_finite() || !width.is_finite() || width <= 0.0 {
        return None;
    }

    let x = gtk_f64_floor_to_i32(x);
    // Include a small upward allowance for font ascent and theme rounding so
    // the detector samples actual minimap glyph pixels.
    let y = gtk_f64_round_to_i32(y).saturating_sub(2);
    let width = gtk_f64_ceil_to_i32(width).max(1);
    Some(AutomationVisualRect {
        x,
        y,
        width,
        height: 18,
    })
}

/// Convert finite projected bounds into outward-rounded logical pixels.
///
/// Outward rounding preserves thin native slider edges for screenshot crops
/// while rejecting non-finite or empty geometry before serialization.
fn visual_rect_from_projected_bounds(
    bounds: crate::ui::editor_page::MinimapProjectedBounds,
) -> Option<AutomationVisualRect> {
    if !bounds.x.is_finite()
        || !bounds.y.is_finite()
        || !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        return None;
    }

    // Round outward so diagnostic crops include the full native slider even
    // when GtkSourceView reports fractional logical coordinates.
    Some(AutomationVisualRect {
        x: gtk_f64_floor_to_i32(bounds.x),
        y: gtk_f64_floor_to_i32(bounds.y),
        width: gtk_f64_ceil_to_i32(bounds.width).max(1),
        height: gtk_f64_ceil_to_i32(bounds.height).max(1),
    })
}

/// Translate minimap availability into stable Automation1 absence reasons.
///
/// Visible-but-unprojectable state reports `bounds-unavailable`, because these
/// strings are smoke-test policy as much as user-interface state.
fn minimap_availability_absence_reason(editor: &LushtextEditorPage) -> &'static str {
    match editor.minimap_availability() {
        crate::ui::editor_page::MinimapAvailability::Disabled => "minimap-disabled",
        crate::ui::editor_page::MinimapAvailability::TooLarge => "minimap-too-large",
        crate::ui::editor_page::MinimapAvailability::Evicted => "minimap-evicted",
        crate::ui::editor_page::MinimapAvailability::Visible => "bounds-unavailable",
    }
}

fn scroll_anchor_snapshot(
    name: &str,
    hadjustment: Option<&gtk4::Adjustment>,
    vadjustment: Option<&gtk4::Adjustment>,
    left_margin_tolerance: f64,
    top_margin_tolerance: f64,
) -> AutomationVisualScrollAnchorSnapshot {
    AutomationVisualScrollAnchorSnapshot {
        name: name.to_string(),
        at_left: hadjustment
            .map(|adjustment| adjustment_at_lower_edge(adjustment, left_margin_tolerance)),
        at_top: vadjustment
            .map(|adjustment| adjustment_at_lower_edge(adjustment, top_margin_tolerance)),
        x_value_milli: hadjustment.map(|adjustment| adjustment_milli(adjustment.value())),
        x_lower_milli: hadjustment.map(|adjustment| adjustment_milli(adjustment.lower())),
        y_value_milli: vadjustment.map(|adjustment| adjustment_milli(adjustment.value())),
        y_lower_milli: vadjustment.map(|adjustment| adjustment_milli(adjustment.lower())),
    }
}

fn adjustment_at_lower_edge(adjustment: &gtk4::Adjustment, tolerance: f64) -> bool {
    adjustment.value() - adjustment.lower() <= tolerance + 0.5
}

fn visual_rect_from_bounds(bounds: gtk4::graphene::Rect) -> Option<AutomationVisualRect> {
    let width = gtk_coord_to_i32(bounds.width());
    let height = gtk_coord_to_i32(bounds.height());
    (width > 0 && height > 0).then_some(AutomationVisualRect {
        x: gtk_coord_to_i32(bounds.x()),
        y: gtk_coord_to_i32(bounds.y()),
        width,
        height,
    })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "GTK widget coordinates are already bounded to practical window-sized logical pixels"
)]
fn gtk_coord_to_i32(value: f32) -> i32 {
    value.round() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "GTK projected coordinates are bounded to practical window-sized logical pixels"
)]
fn gtk_f64_floor_to_i32(value: f64) -> i32 {
    value.floor() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "GTK projected coordinates are bounded to practical window-sized logical pixels"
)]
fn gtk_f64_round_to_i32(value: f64) -> i32 {
    value.round() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "GTK projected dimensions are bounded to practical window-sized logical pixels"
)]
fn gtk_f64_ceil_to_i32(value: f64) -> i32 {
    value.ceil() as i32
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "Scroll adjustment values are bounded by GTK widget geometry before conversion to milli-pixels"
)]
fn adjustment_milli(value: f64) -> i64 {
    (value * 1000.0).round() as i64
}

/// Summarize shell surface visibility and requested state after layout mediation.
fn surface_snapshot(window: &LushtextWindow) -> AutomationSurfaceSnapshot {
    let imp = window.imp();
    let document_properties_visible =
        if imp.properties_layout_view.layout_name().as_deref() == Some("sheet") {
            imp.properties_bottom_sheet.is_open()
        } else {
            imp.properties_split_view.shows_sidebar()
        };
    let active_editor_search =
        active_editor(window).is_some_and(|editor| editor.is_search_visible());
    let accessibility_blocker =
        window_readiness_blocker(window, AutomationReadinessPredicate::AccessibilitySettled)
            .map(ToOwned::to_owned);
    let accessibility_ready = accessibility_blocker.is_none();

    AutomationSurfaceSnapshot {
        workspace_sidebar_visible: imp.workspace_split_view.shows_sidebar(),
        workspace_sidebar_requested: imp.secondary_surfaces.workspace_requested_visible.get(),
        document_properties_visible,
        document_properties_requested: imp.secondary_surfaces.properties_requested_visible.get(),
        compact_surface: window.compact_surface_label().map(ToOwned::to_owned),
        command_palette_visible: imp.palette_revealer.reveals_child(),
        search_panel_visible: imp.search_panel_revealer.reveals_child(),
        open_popover_visible: open_popover_visible(window),
        preview_pane_visible: imp.preview_visible.get(),
        preview_mode: imp.preview_mode.get(),
        focus_mode: imp.focus_mode.active.get(),
        minimap_requested: imp.settings.boolean(config::keys::SHOW_MINIMAP),
        status_bar_visible: imp.status_bar.is_visible(),
        active_transient_surface: active_transient_surface_name(
            imp.palette_revealer.reveals_child(),
            imp.search_panel_revealer.reveals_child(),
            open_popover_visible(window),
            active_editor_search,
        ),
        accessibility_ready,
        accessibility_blocker,
    }
}

/// Summarize in-document and workspace-search state with bounded query fields.
fn search_snapshot(window: &LushtextWindow) -> AutomationSearchSnapshot {
    let imp = window.imp();
    let editor = active_editor(window);
    let editor_search_visible = editor
        .as_ref()
        .is_some_and(LushtextEditorPage::is_search_visible);
    let editor_query = editor.as_ref().and_then(|editor| {
        editor_search_visible
            .then(|| bounded_snapshot_text(editor.search_bar().search_entry().text()))
    });
    let editor_match_count = editor.as_ref().and_then(|editor| {
        editor
            .search_bar()
            .search_context()
            .map(|context| context.occurrences_count())
    });

    AutomationSearchSnapshot {
        editor_search_visible,
        editor_query,
        editor_match_count,
        workspace_search_visible: imp.search_panel_revealer.reveals_child(),
        workspace_query: bounded_snapshot_text(imp.search_panel.query()),
        workspace_searching: imp.search_panel.is_searching(),
        workspace_match_count: imp.search_panel.total_matches(),
        workspace_file_count: imp.search_panel.total_files(),
        workspace_result_capped: imp.search_panel.result_capped(),
    }
}

fn active_editor(window: &LushtextWindow) -> Option<LushtextEditorPage> {
    window
        .imp()
        .tab_view
        .selected_page()
        .and_then(|page| page.child().downcast::<LushtextEditorPage>().ok())
}

fn load_state_name(state: EditorLoadState) -> &'static str {
    match state {
        EditorLoadState::Untitled => "untitled",
        EditorLoadState::Loading => "loading",
        EditorLoadState::Loaded => "loaded",
        EditorLoadState::Failed => "failed",
    }
}

fn workspace_scope_name(scope: &WorkspaceScope) -> &'static str {
    match scope {
        WorkspaceScope::All => "all",
        WorkspaceScope::Workspace(_) => "workspace",
    }
}

fn search_mode_name(mode: SearchMode) -> &'static str {
    mode.stable_name()
}

fn local_history_availability_name(availability: LocalHistoryAvailability) -> &'static str {
    match availability {
        LocalHistoryAvailability::Full => "full",
        LocalHistoryAvailability::SaveOnly => "save-only",
        LocalHistoryAvailability::Unavailable => "unavailable",
    }
}

fn notification_severity_name(severity: NotificationSeverity) -> &'static str {
    match severity {
        NotificationSeverity::Info => "info",
        NotificationSeverity::Warning => "warning",
        NotificationSeverity::Error => "error",
    }
}

fn bounded_len(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

fn bounded_snapshot_text(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    if text.len() <= SNAPSHOT_TEXT_MAX_BYTES {
        return text.to_string();
    }

    let content_limit = SNAPSHOT_TEXT_MAX_BYTES.saturating_sub(SNAPSHOT_TRUNCATION_MARKER.len());
    let boundary = text.floor_char_boundary(content_limit);
    let mut bounded = String::with_capacity(boundary + SNAPSHOT_TRUNCATION_MARKER.len());
    bounded.push_str(&text[..boundary]);
    bounded.push_str(SNAPSHOT_TRUNCATION_MARKER);
    bounded
}

fn active_transient_surface_name(
    command_palette_visible: bool,
    search_panel_visible: bool,
    open_popover_visible: bool,
    editor_search_visible: bool,
) -> Option<String> {
    if command_palette_visible {
        Some("command-palette".to_string())
    } else if search_panel_visible {
        Some("workspace-search".to_string())
    } else if open_popover_visible {
        Some("open-popover".to_string())
    } else if editor_search_visible {
        Some("editor-search".to_string())
    } else {
        None
    }
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn introspection_xml_declares_version_properties_and_snapshot_methods() {
        let node = gio::DBusNodeInfo::for_xml(INTROSPECTION_XML)
            .expect("automation introspection XML should parse");
        let interface = node
            .lookup_interface(INTERFACE_NAME)
            .expect("automation interface should exist");

        for property in ["InterfaceVersion", "Enabled", "BuildProfile"] {
            assert!(
                interface.lookup_property(property).is_some(),
                "{property} should be declared"
            );
        }
        for (property, signature) in [
            ("InterfaceVersion", "type='u'"),
            ("Enabled", "type='b'"),
            ("BuildProfile", "type='s'"),
        ] {
            let fragment = format!("<property name='{property}' {signature} access='read'/>");
            assert!(
                INTROSPECTION_XML.contains(&fragment),
                "{property} should keep signature fragment {fragment}"
            );
        }

        for method in [
            "GetActionCatalog",
            "GetSnapshot",
            "GetReadinessPredicates",
            "GetWorkflowEvents",
            "WaitForReady",
            "WaitForIdle",
        ] {
            assert!(
                interface.lookup_method(method).is_some(),
                "{method} should be declared"
            );
        }
        for method in [
            "GetActionCatalog",
            "GetSnapshot",
            "GetReadinessPredicates",
            "GetWorkflowEvents",
        ] {
            let fragment = format!(
                "<method name='{method}'>\n      <arg type='s' name='json' direction='out'/>"
            );
            assert!(
                INTROSPECTION_XML.contains(&fragment),
                "{method} should return one JSON string"
            );
        }
        assert!(INTROSPECTION_XML.contains(
            "<method name='WaitForReady'>\n      <arg type='s' name='predicate' direction='in'/>\n      <arg type='u' name='timeout_msec' direction='in'/>\n      <arg type='b' name='ok' direction='out'/>\n      <arg type='s' name='status' direction='out'/>\n      <arg type='s' name='detail' direction='out'/>"
        ));
        assert!(INTROSPECTION_XML.contains(
            "<method name='WaitForIdle'>\n      <arg type='u' name='timeout_msec' direction='in'/>\n      <arg type='b' name='ok' direction='out'/>\n      <arg type='s' name='detail' direction='out'/>"
        ));
    }

    #[test]
    fn introspection_xml_matches_stable_automation1_golden() {
        let normalized = INTROSPECTION_XML
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let expected = "\
<node>
<interface name='dev.cominotti.lushtext.Automation1'>
<property name='InterfaceVersion' type='u' access='read'/>
<property name='Enabled' type='b' access='read'/>
<property name='BuildProfile' type='s' access='read'/>
<method name='GetActionCatalog'>
<arg type='s' name='json' direction='out'/>
</method>
<method name='GetSnapshot'>
<arg type='s' name='json' direction='out'/>
</method>
<method name='GetReadinessPredicates'>
<arg type='s' name='json' direction='out'/>
</method>
<method name='GetWorkflowEvents'>
<arg type='s' name='json' direction='out'/>
</method>
<method name='WaitForReady'>
<arg type='s' name='predicate' direction='in'/>
<arg type='u' name='timeout_msec' direction='in'/>
<arg type='b' name='ok' direction='out'/>
<arg type='s' name='status' direction='out'/>
<arg type='s' name='detail' direction='out'/>
</method>
<method name='WaitForIdle'>
<arg type='u' name='timeout_msec' direction='in'/>
<arg type='b' name='ok' direction='out'/>
<arg type='s' name='detail' direction='out'/>
</method>
</interface>
</node>";

        assert_eq!(normalized, expected);
    }

    #[test]
    fn automation_object_path_is_child_of_application_object_path() {
        assert_eq!(
            object_path_for("/dev/cominotti/lushtext"),
            "/dev/cominotti/lushtext/Automation"
        );
    }

    #[test]
    fn bounded_snapshot_text_caps_free_form_fields_without_splitting_utf8() {
        let oversized = format!(
            "{}é",
            "a".repeat(SNAPSHOT_TEXT_MAX_BYTES + SNAPSHOT_TRUNCATION_MARKER.len())
        );

        let bounded = bounded_snapshot_text(oversized);

        assert!(bounded.len() <= SNAPSHOT_TEXT_MAX_BYTES);
        assert!(bounded.ends_with(SNAPSHOT_TRUNCATION_MARKER));
        assert!(bounded.is_char_boundary(bounded.len()));
    }
}
