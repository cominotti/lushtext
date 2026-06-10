## ADDED Requirements

### Requirement: RAII signal lifetime bags
`gtk-lush-signals` SHALL provide value types (`SignalBag` and related typed
helpers) that record GObject signal handler registrations and disconnect every
recorded handler exactly once when cleared or dropped. Recording SHALL support
the common shapes used by composite widgets: handlers on owned child widgets,
handlers on shared long-lived objects (for example `gio::Settings` and
`adw::StyleManager`) held via weak source references, and GTK event
controllers. Disconnect-on-drop MUST be safe when the source object has
already been finalized.

#### Scenario: Handlers disconnect on drop
- **WHEN** a widget stores its handler registrations in a `SignalBag` and the
  bag is dropped during dispose
- **THEN** every recorded handler is disconnected exactly once and no handler
  closure runs afterward

#### Scenario: Shared-object handlers do not leak widgets
- **WHEN** a handler on an application-global object is recorded with a bag
  owned by a widget, and the widget is destroyed
- **THEN** the handler is disconnected and the widget is finalized (no strong
  reference cycle keeps it alive)

#### Scenario: Already-finalized source is tolerated
- **WHEN** the bag drops after a recorded source object has been finalized
- **THEN** clearing skips the dead registration without panicking or logging
  GLib criticals

### Requirement: Binding lifetime management
The crate SHALL provide the same RAII ownership for `glib::Binding` property
bindings, unbinding on clear/drop, and supporting the one-way and two-way
binding shapes LushText uses for settings and widget state.

#### Scenario: Bindings unbind with the bag
- **WHEN** a property binding recorded in a bag is dropped with the bag
- **THEN** the binding is unbound and subsequent source changes no longer
  propagate

### Requirement: LushText migration with behavior preserved
LushText SHALL migrate its manual handler-id bookkeeping (the
`RefCell<Option<glib::SignalHandlerId>>` fields and matching dispose/Drop
disconnect blocks across `editor_page`, `window`, `sidebar`, and preference
bindings) onto the crate, deleting the per-field bookkeeping rather than
wrapping it. Migration MUST NOT change observable behavior: the full widget
suite, non-widget suites, and the GTK warning gate stay green, and no
GLib-GObject warnings are introduced.

#### Scenario: Migration keeps the gates green
- **WHEN** a LushText module's handler bookkeeping is replaced by signal bags
- **THEN** the full widget suite passes headless with zero unexpected
  GTK/GLib warnings and the module's handler-id fields are removed

#### Scenario: Rules point at crate docs
- **WHEN** the migration completes
- **THEN** the relevant `.agents/rules` sections describe the crate as the
  required mechanism and retain only LushText-specific judgment
