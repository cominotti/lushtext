// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure action-catalog value objects used by automation docs and audits.
//!
//! The catalog is a model-layer contract: it has no GTK dependency and can be
//! serialized, tested, and consumed by smoke helpers without constructing a
//! window.

use std::borrow::Cow;

use serde::Serialize;

/// Scope prefix that owns an action identifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionScope {
    /// Application-level action exported by `LushtextApplication`.
    App,
    /// Window-level action exported by each `LushtextWindow`.
    Window,
    /// Search-bar options popover action group.
    SearchOptions,
    /// Per-workspace-section file or folder context menu action group.
    SidebarSection,
    /// Per-workspace-section header context menu action group.
    WorkspaceHeader,
}

impl ActionScope {
    /// Return the action prefix used by GTK action strings such as `win.save`.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Window => "win",
            Self::SearchOptions => "search-options",
            Self::SidebarSection => "section",
            Self::WorkspaceHeader => "ws-header",
        }
    }

    /// Build the fully-qualified action id used by menus and documentation.
    #[must_use]
    pub fn qualified_id(self, name: &str) -> String {
        format!("{}.{}", self.prefix(), name)
    }
}

/// GLib action parameter or state type recorded in the catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionValueType {
    /// No parameter or state value is exposed.
    None,
    /// Boolean state or parameter (`b` in GVariant signatures).
    Bool,
    /// UTF-8 string value (`s` in GVariant signatures).
    String,
    /// Unsigned 32-bit integer value (`u` in GVariant signatures).
    U32,
    /// Variant dictionary (`a{sv}`) for future structured GTK action parameters.
    ///
    /// This does not imply mutating methods on the custom Automation1 D-Bus
    /// object, which remains read-only.
    VariantMap,
}

impl ActionValueType {
    /// Return the matching GVariant signature when the action has a value.
    #[must_use]
    pub const fn glib_signature(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Bool => Some("b"),
            Self::String => Some("s"),
            Self::U32 => Some("u"),
            Self::VariantMap => Some("a{sv}"),
        }
    }
}

/// How a cataloged action is exposed by the running application today.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionExposure {
    /// The action is exported by app/window `org.gtk.Actions`.
    Exported,
    /// The action belongs to a widget-local action group resolved by GTK menus.
    WidgetScoped,
    /// The action is visible in UI metadata but is not registered yet.
    VisibleUnregisteredGap,
}

/// Safety classification for externally activating a cataloged action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalActivationSafety {
    /// Safe for external tools to activate as a normal user command.
    StableUserCommand,
    /// Safe, but the command depends on active document, workspace, or menu context.
    ContextualUserCommand,
    /// Exported for diagnostics but not currently a visible user command.
    DiagnosticOnly,
    /// Visible command metadata exists, but there is no working action yet.
    UnsupportedGap,
}

/// User-visible surface or automation surface that references an action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionSurface {
    HeaderButton,
    PrimaryMenu,
    NotesMenu,
    StatusBar,
    PropertiesPanel,
    EditorContextMenu,
    SearchOptionsMenu,
    TabContextMenu,
    SidebarFileContextMenu,
    SidebarFolderContextMenu,
    WorkspaceHeaderContextMenu,
    KeyboardShortcut,
    CommandPalette,
    /// Externally invokable through GTK's normal `org.gtk.Actions` surface.
    ///
    /// The custom Automation1 D-Bus object only documents and observes these
    /// actions; mutations still flow through GTK action activation.
    DbusAction,
    CustomMenuWidget,
}

/// Test or smoke lane that proves an action remains wired correctly.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionCoverageLane {
    Unit,
    Widget,
    DbusSmoke,
    VisualSmoke,
    AccessibilitySmoke,
    ManualDiagnostic,
}

/// One catalog row for a public, contextual, or documented-gap action.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ActionCatalogEntry {
    /// Action group that owns the GTK action prefix.
    pub scope: ActionScope,
    /// Unqualified action name, for example `save` in `win.save`.
    pub name: &'static str,
    /// Human-readable label used in visible menus or generated references.
    pub label: &'static str,
    /// Expected GVariant parameter type for activation.
    pub parameter_type: ActionValueType,
    /// Expected GVariant state type for stateful actions.
    pub state_type: ActionValueType,
    /// Short rule describing when the action is meaningful or enabled.
    pub enablement: &'static str,
    /// Source module or workflow responsible for registering/handling the action.
    pub owner: &'static str,
    /// User-visible or automation surfaces that expose the action.
    pub surfaces: &'static [ActionSurface],
    /// Whether external same-user tools should treat activation as stable, contextual, diagnostic, or unsupported.
    pub external_activation: ExternalActivationSafety,
    /// How the action is exposed by the running app today.
    pub exposure: ActionExposure,
    /// Stable documentation anchor in `docs/automation-reference.md`.
    pub docs_anchor: &'static str,
    /// Test or smoke lanes that currently cover the action wiring.
    pub coverage_lanes: &'static [ActionCoverageLane],
}

impl ActionCatalogEntry {
    /// Construct a static catalog entry without heap allocation.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "catalog rows intentionally name every documented automation contract field"
    )]
    pub const fn new(
        scope: ActionScope,
        name: &'static str,
        label: &'static str,
        parameter_type: ActionValueType,
        state_type: ActionValueType,
        enablement: &'static str,
        owner: &'static str,
        surfaces: &'static [ActionSurface],
        external_activation: ExternalActivationSafety,
        exposure: ActionExposure,
        docs_anchor: &'static str,
        coverage_lanes: &'static [ActionCoverageLane],
    ) -> Self {
        Self {
            scope,
            name,
            label,
            parameter_type,
            state_type,
            enablement,
            owner,
            surfaces,
            external_activation,
            exposure,
            docs_anchor,
            coverage_lanes,
        }
    }

    /// Return the action id in the same `prefix.name` form used by GTK menus.
    #[must_use]
    pub fn qualified_id(&self) -> String {
        self.scope.qualified_id(self.name)
    }
}

/// Runtime or test-observed action signature used by catalog audits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedAction<'a> {
    /// Action group that owns the observed GTK action prefix.
    pub scope: ActionScope,
    /// Unqualified observed action name.
    pub name: Cow<'a, str>,
    /// Runtime GVariant parameter type reported by GTK.
    pub parameter_type: ActionValueType,
    /// Runtime GVariant state type reported by GTK.
    pub state_type: ActionValueType,
}

impl<'a> ObservedAction<'a> {
    /// Construct a compact observed signature for an action-introspection audit.
    #[must_use]
    pub const fn new(
        scope: ActionScope,
        name: &'static str,
        parameter_type: ActionValueType,
        state_type: ActionValueType,
    ) -> Self {
        Self {
            scope,
            name: Cow::Borrowed(name),
            parameter_type,
            state_type,
        }
    }

    /// Construct an observed signature from a live GTK action name.
    #[must_use]
    pub fn owned(
        scope: ActionScope,
        name: String,
        parameter_type: ActionValueType,
        state_type: ActionValueType,
    ) -> Self {
        Self {
            scope,
            name: Cow::Owned(name),
            parameter_type,
            state_type,
        }
    }

    /// Return the fully-qualified action id for deterministic audit messages.
    #[must_use]
    pub fn qualified_id(&self) -> String {
        self.scope.qualified_id(&self.name)
    }
}

/// Serializable row consumed by the developer automation reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ActionReferenceRow {
    /// Fully-qualified action id used in docs, for example `win.save`.
    pub action_id: String,
    /// Action group that owns the GTK action prefix.
    pub scope: ActionScope,
    /// Unqualified action name.
    pub name: &'static str,
    /// Human-readable label used by the reference table.
    pub label: &'static str,
    /// Cataloged parameter value kind.
    pub parameter_type: ActionValueType,
    /// Matching GVariant parameter signature, if any.
    pub parameter_signature: Option<&'static str>,
    /// Cataloged state value kind.
    pub state_type: ActionValueType,
    /// Matching GVariant state signature, if any.
    pub state_signature: Option<&'static str>,
    /// Human-readable rule describing when the action applies.
    pub enablement: &'static str,
    /// Source module or workflow responsible for the action.
    pub owner: &'static str,
    /// Surfaces that expose this action.
    pub surfaces: Vec<ActionSurface>,
    /// Same-user automation safety classification.
    pub external_activation: ExternalActivationSafety,
    /// Runtime exposure classification.
    pub exposure: ActionExposure,
    /// Stable `docs/automation-reference.md` anchor.
    pub docs_anchor: &'static str,
    /// Test or smoke lanes that cover the action.
    pub coverage_lanes: Vec<ActionCoverageLane>,
}

impl From<&ActionCatalogEntry> for ActionReferenceRow {
    fn from(entry: &ActionCatalogEntry) -> Self {
        Self {
            action_id: entry.qualified_id(),
            scope: entry.scope,
            name: entry.name,
            label: entry.label,
            parameter_type: entry.parameter_type,
            parameter_signature: entry.parameter_type.glib_signature(),
            state_type: entry.state_type,
            state_signature: entry.state_type.glib_signature(),
            enablement: entry.enablement,
            owner: entry.owner,
            surfaces: entry.surfaces.to_vec(),
            external_activation: entry.external_activation,
            exposure: entry.exposure,
            docs_anchor: entry.docs_anchor,
            coverage_lanes: entry.coverage_lanes.to_vec(),
        }
    }
}
