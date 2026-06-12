// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal Automation1 client for live visual proof orchestration.
//!
//! The visual runner must drive LushText through the same public surface used by
//! developers and smoke helpers: read-only Automation1 snapshots/readiness plus
//! normal `org.gtk.Actions` activations. This module keeps that D-Bus boundary
//! small and independently testable so runner code can record bounded evidence
//! without learning GVariant details.

#![allow(
    dead_code,
    reason = "Automation1 client lands before live-runner orchestration starts using every call path"
)]

use std::thread;
use std::time::{Duration, Instant};

use gio::prelude::DBusProxyExt;
use serde::Serialize;
use serde_json::Value;

const APP_BUS_NAME: &str = "dev.cominotti.lushtext";
const AUTOMATION_OBJECT_PATH: &str = "/dev/cominotti/lushtext/Automation";
const AUTOMATION_INTERFACE: &str = "dev.cominotti.lushtext.Automation1";
const INTROSPECTABLE_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";
const WINDOW_OBJECT_PATH: &str = "/dev/cominotti/lushtext/window/1";
const ACTIONS_INTERFACE: &str = "org.gtk.Actions";
const ARTIFACT_TEXT_LIMIT: usize = 1200;

/// Status returned by an Automation1 readiness wait.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct ReadinessWait {
    /// Predicate that was requested.
    pub(crate) predicate: String,
    /// Whether Automation1 reported readiness before the timeout.
    pub(crate) ok: bool,
    /// Stable Automation1 readiness status.
    pub(crate) status: String,
    /// Bounded detail from the app-owned readiness path.
    pub(crate) detail: String,
}

/// Bounded row written to case artifacts after a D-Bus read or action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AutomationArtifactRow {
    /// Operation name, such as `GetSnapshot`, `WaitForReady`, or `win.toggle-sidebar`.
    pub(crate) name: String,
    /// Stable row status suitable for per-case manifests.
    pub(crate) status: String,
    /// Bounded diagnostic detail.
    pub(crate) detail: String,
    /// Optional relative artifact path holding the richer payload.
    pub(crate) artifact: Option<String>,
}

/// Synchronous Automation1 D-Bus client used by the live runner.
pub(crate) struct AutomationClient {
    automation: gio::DBusProxy,
    automation_introspection: gio::DBusProxy,
    window_actions: gio::DBusProxy,
}

impl AutomationClient {
    /// Connect and verify the app has exported window actions and Automation1.
    pub(crate) fn connect_with_retry(timeout: Duration) -> Result<Self, String> {
        let deadline = Instant::now() + timeout;
        let mut last_error = "Automation1 not probed yet".to_string();
        while Instant::now() < deadline {
            match Self::new(1_000) {
                Ok(client) => {
                    let actions = client.window_actions_available();
                    let automation = client.introspect();
                    if actions.is_ok() && automation.is_ok() {
                        return Ok(client);
                    }
                    last_error = actions
                        .err()
                        .or_else(|| automation.err())
                        .unwrap_or_else(|| "Automation1 was not ready".to_string());
                }
                Err(error) => last_error = error,
            }
            thread::sleep(Duration::from_millis(100));
        }
        Err(format!(
            "LushText did not export window actions and Automation1 before timeout: {last_error}"
        ))
    }

    /// Connect to the app-owned Automation1 object and window action group.
    pub(crate) fn new(timeout_msec: i32) -> Result<Self, String> {
        let flags = gio::DBusProxyFlags::DO_NOT_AUTO_START;
        let automation = gio::DBusProxy::for_bus_sync(
            gio::BusType::Session,
            flags,
            None::<&gio::DBusInterfaceInfo>,
            APP_BUS_NAME,
            AUTOMATION_OBJECT_PATH,
            AUTOMATION_INTERFACE,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("cannot connect to Automation1: {error}"))?;
        automation.set_default_timeout(timeout_msec);
        let automation_introspection = gio::DBusProxy::for_bus_sync(
            gio::BusType::Session,
            flags,
            None::<&gio::DBusInterfaceInfo>,
            APP_BUS_NAME,
            AUTOMATION_OBJECT_PATH,
            INTROSPECTABLE_INTERFACE,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("cannot connect to Automation1 introspection: {error}"))?;
        automation_introspection.set_default_timeout(timeout_msec);
        let window_actions = gio::DBusProxy::for_bus_sync(
            gio::BusType::Session,
            flags,
            None::<&gio::DBusInterfaceInfo>,
            APP_BUS_NAME,
            WINDOW_OBJECT_PATH,
            ACTIONS_INTERFACE,
            gio::Cancellable::NONE,
        )
        .map_err(|error| format!("cannot connect to window actions: {error}"))?;
        window_actions.set_default_timeout(timeout_msec);
        Ok(Self {
            automation,
            automation_introspection,
            window_actions,
        })
    }

    /// Read and parse the Automation1 action catalog.
    pub(crate) fn action_catalog(&self) -> Result<Value, String> {
        self.call_json_method("GetActionCatalog")
    }

    /// Read and parse the Automation1 introspection XML.
    pub(crate) fn introspect(&self) -> Result<String, String> {
        let result = self
            .automation_introspection
            .call_sync(
                "Introspect",
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            )
            .map_err(|error| format!("Automation1 Introspect failed: {error}"))?;
        parse_single_string_result(&result)
            .ok_or_else(|| "Introspect returned an unexpected D-Bus tuple".to_string())
    }

    /// Read and parse the Automation1 readiness predicate metadata.
    pub(crate) fn readiness_predicates(&self) -> Result<Value, String> {
        self.call_json_method("GetReadinessPredicates")
    }

    /// Read and parse a bounded Automation1 snapshot.
    pub(crate) fn snapshot(&self) -> Result<Value, String> {
        self.call_json_method("GetSnapshot")
    }

    /// Read and parse the recent workflow-event snapshot.
    pub(crate) fn workflow_events(&self) -> Result<Value, String> {
        self.call_json_method("GetWorkflowEvents")
    }

    /// Wait for a named readiness predicate and return the stable result row.
    pub(crate) fn wait_for_ready(
        &self,
        predicate: &str,
        timeout_msec: u32,
    ) -> Result<ReadinessWait, String> {
        let parameters = glib::Variant::from((predicate, timeout_msec));
        let result =
            self.call_automation("WaitForReady", Some(&parameters), timeout_msec as i32)?;
        parse_wait_for_ready_result(predicate, &result)
    }

    /// Activate one exported window action through `org.gtk.Actions`.
    pub(crate) fn activate_window_action(
        &self,
        action: &str,
        parameter: ActionParameter<'_>,
    ) -> Result<AutomationArtifactRow, String> {
        let parameters = action_activation_parameters(action, parameter)?;
        self.window_actions
            .call_sync(
                "Activate",
                Some(&parameters),
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            )
            .map_err(|error| format!("cannot activate window action {action}: {error}"))?;
        Ok(AutomationArtifactRow {
            name: action.to_string(),
            status: "passed".to_string(),
            detail: bounded(format!("activated window action {action}")),
            artifact: None,
        })
    }

    fn window_actions_available(&self) -> Result<(), String> {
        self.window_actions
            .call_sync(
                "List",
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            )
            .map(|_| ())
            .map_err(|error| format!("window actions are not available: {error}"))
    }

    fn call_json_method(&self, method: &str) -> Result<Value, String> {
        let result = self.call_automation(method, None, -1)?;
        let json = parse_single_string_result(&result)
            .ok_or_else(|| format!("{method} returned an unexpected D-Bus tuple"))?;
        let value = serde_json::from_str::<Value>(&json)
            .map_err(|error| format!("{method} returned invalid JSON: {error}"))?;
        validate_snapshot_privacy(&value).map_err(|field| {
            format!("{method} exposed private or unbounded automation field: {field}")
        })?;
        Ok(value)
    }

    fn call_automation(
        &self,
        method: &str,
        parameters: Option<&glib::Variant>,
        timeout_msec: i32,
    ) -> Result<glib::Variant, String> {
        self.automation
            .call_sync(
                method,
                parameters,
                gio::DBusCallFlags::NONE,
                timeout_msec,
                gio::Cancellable::NONE,
            )
            .map_err(|error| format!("Automation1 {method} failed: {error}"))
    }
}

/// Supported GTK action parameter shapes needed by visual proof scenarios.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionParameter<'a> {
    /// No parameter.
    None,
    /// One string parameter.
    String(&'a str),
    /// One boolean parameter.
    Bool(bool),
}

fn action_activation_parameters(
    action: &str,
    parameter: ActionParameter<'_>,
) -> Result<glib::Variant, String> {
    let variants = match parameter {
        ActionParameter::None => "[]".to_string(),
        ActionParameter::String(value) => format!("[<{}>]", glib_variant_string(value)),
        ActionParameter::Bool(value) => format!("[<{}>]", if value { "true" } else { "false" }),
    };
    let text = format!("({}, {variants}, {{}})", glib_variant_string(action));
    let variant_type = glib::VariantTy::new("(sava{sv})")
        .map_err(|error| format!("invalid action activation variant type: {error}"))?;
    glib::Variant::parse(Some(variant_type), &text)
        .map_err(|error| format!("cannot build action activation variant for {action}: {error}"))
}

fn glib_variant_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('\'', "\\'");
    format!("'{escaped}'")
}

fn parse_single_string_result(result: &glib::Variant) -> Option<String> {
    let child = result.child_value(0);
    child.str().map(ToOwned::to_owned)
}

fn parse_wait_for_ready_result(
    predicate: &str,
    result: &glib::Variant,
) -> Result<ReadinessWait, String> {
    let ok = result
        .child_value(0)
        .get::<bool>()
        .ok_or_else(|| "WaitForReady missing boolean ok field".to_string())?;
    let status_value = result.child_value(1);
    let status = status_value
        .str()
        .ok_or_else(|| "WaitForReady missing status field".to_string())?;
    let detail_value = result.child_value(2);
    let detail = detail_value
        .str()
        .ok_or_else(|| "WaitForReady missing detail field".to_string())?;
    Ok(ReadinessWait {
        predicate: predicate.to_string(),
        ok,
        status: status.to_string(),
        detail: bounded(detail),
    })
}

fn validate_snapshot_privacy(value: &Value) -> Result<(), String> {
    validate_snapshot_privacy_at(value, "$")
}

fn validate_snapshot_privacy_at(value: &Value, path: &str) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                let child = format!("{path}.{key}");
                if private_field_name(key) {
                    return Err(child);
                }
                validate_snapshot_privacy_at(value, &child)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                validate_snapshot_privacy_at(value, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::String(text) if text.len() > ARTIFACT_TEXT_LIMIT => Err(path.to_string()),
        _ => Ok(()),
    }
}

fn private_field_name(key: &str) -> bool {
    let normalized = key.replace(['-', '_'], "");
    [
        "documenttext",
        "notebody",
        "draftbody",
        "localhistorycontents",
        "searchresulttext",
        "sidecaridentity",
        "privatepersistenceidentifier",
    ]
    .iter()
    .any(|forbidden| normalized.contains(forbidden))
}

fn bounded(text: impl AsRef<str>) -> String {
    let text = text.as_ref();
    if text.len() <= ARTIFACT_TEXT_LIMIT {
        text.to_string()
    } else {
        let suffix = " [truncated]";
        let target_len = ARTIFACT_TEXT_LIMIT.saturating_sub(suffix.len());
        let end = text
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= target_len)
            .last()
            .unwrap_or(0);
        format!("{}{}", &text[..end], suffix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_for_ready_tuple_is_parsed_and_bounded() {
        let long_detail = "x".repeat(ARTIFACT_TEXT_LIMIT + 10);
        let variant = glib::Variant::from((false, "predicate-timeout", long_detail.as_str()));

        let wait =
            parse_wait_for_ready_result("visual-geometry-settled", &variant).expect("parse wait");

        assert!(!wait.ok);
        assert_eq!(wait.predicate, "visual-geometry-settled");
        assert_eq!(wait.status, "predicate-timeout");
        assert!(wait.detail.ends_with(" [truncated]"));
        assert!(wait.detail.len() < long_detail.len());
    }

    #[test]
    fn wait_for_ready_unknown_predicate_is_preserved() {
        let variant = glib::Variant::from((false, "unknown-predicate", "no such predicate"));

        let wait = parse_wait_for_ready_result("missing-ready", &variant).expect("parse wait");

        assert!(!wait.ok);
        assert_eq!(wait.predicate, "missing-ready");
        assert_eq!(wait.status, "unknown-predicate");
        assert_eq!(wait.detail, "no such predicate");
    }

    #[test]
    fn snapshot_privacy_rejects_private_fields_and_unbounded_text() {
        let private = serde_json::json!({
            "window": {
                "note_body": "secret"
            }
        });
        assert!(
            validate_snapshot_privacy(&private)
                .expect_err("private snapshot field should fail")
                .contains("note_body")
        );

        let unbounded = serde_json::json!({
            "window": {
                "status_message": "x".repeat(ARTIFACT_TEXT_LIMIT + 1)
            }
        });
        assert!(
            validate_snapshot_privacy(&unbounded)
                .expect_err("unbounded snapshot text should fail")
                .contains("status_message")
        );
    }

    #[test]
    fn snapshot_privacy_accepts_visual_geometry_subset() {
        let snapshot = serde_json::json!({
            "idle": true,
            "window": {
                "visual_geometry": {
                    "ready": true,
                    "surfaces": [{
                        "name": "workspace-sidebar",
                        "rect": {"x": 0, "y": 0, "width": 320, "height": 720}
                    }],
                    "pixel_anchors": [{
                        "name": "minimap-native-viewport-top-edge",
                        "screen_rect": {"x": 10, "y": 10, "width": 20, "height": 5}
                    }]
                }
            }
        });

        validate_snapshot_privacy(&snapshot).expect("safe snapshot");
    }

    #[test]
    fn action_activation_parameters_match_org_gtk_actions_signature() {
        for (parameter, expected_text) in [
            (ActionParameter::None, "('toggle-sidebar', [], {})"),
            (
                ActionParameter::String("needle"),
                "('set-search-query', [<'needle'>], {})",
            ),
            (
                ActionParameter::Bool(true),
                "('set-sidebar-visible', [<true>], {})",
            ),
        ] {
            let action = match parameter {
                ActionParameter::None => "toggle-sidebar",
                ActionParameter::String(_) => "set-search-query",
                ActionParameter::Bool(_) => "set-sidebar-visible",
            };
            let variant =
                action_activation_parameters(action, parameter).expect("activation parameters");

            assert_eq!(variant.type_().as_str(), "(sava{sv})");
            assert_eq!(variant.print(false).as_str(), expected_text);
        }
    }

    #[test]
    fn action_activation_parameters_escape_strings() {
        let variant =
            action_activation_parameters("set-search-query", ActionParameter::String("a'b\\c"))
                .expect("activation parameters");

        assert_eq!(variant.type_().as_str(), "(sava{sv})");
        assert!(variant.print(false).contains("a'b\\\\c"));
    }
}
