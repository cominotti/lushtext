## MODIFIED Requirements

### Requirement: Advertised Document MIME Types
LushText SHALL advertise exactly the curated document MIME types needed for GNOME's standard file association UI: `text/plain`, `application/x-zerosize`, `application/json`, `application/json5`, `application/toml`, `application/yaml`, and `text/markdown`. The advertised desktop metadata MUST NOT include programming-language source MIME types such as `text/x-csrc`, `text/x-chdr`, `text/x-python`, or `text/x-rust`, and MUST NOT include JSONC or Properties MIME strings as part of this change.

#### Scenario: Shipped desktop entry declares only curated document handlers
- **WHEN** the packaged LushText desktop entry is generated
- **THEN** its `MimeType` field is exactly `text/plain;application/x-zerosize;application/json;application/json5;application/toml;application/yaml;text/markdown;`
- **AND** it does not include `text/x-csrc`, `text/x-chdr`, `text/x-python`, or `text/x-rust`
- **AND** it does not include JSONC or Properties MIME strings

#### Scenario: Development desktop entry mirrors only curated document handlers
- **WHEN** the local development desktop entry is staged for GNOME Shell and Settings integration
- **THEN** its `MimeType` field includes the same exact curated document MIME types as the packaged desktop entry
- **AND** it does not include programming-language source MIME types
- **AND** it does not include JSONC or Properties MIME strings

### Requirement: GNOME Files And Links Visibility
LushText SHALL appear in GNOME/GLib application association data as a registered and recommended handler for Plain Text, Empty, JSON, JSON5, TOML, YAML, and Markdown document file types after its desktop entry is installed or staged and the relevant desktop MIME cache is refreshed. When the Flatpak is installed, this handler visibility SHALL be provided through the Flatpak-backed `dev.cominotti.lushtext.desktop` app info rather than through a stale same-ID non-Flatpak development desktop entry. GLib may still report LushText for some `text/plain` subclasses through inherited parent-type association; that inherited behavior MUST NOT be treated as explicit advertisement in LushText's GNOME Settings File Types contract.

#### Scenario: Plain text is registered and recommended
- **WHEN** GLib queries recommended applications for `text/plain`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: Empty document is registered and recommended
- **WHEN** GLib queries recommended applications for `application/x-zerosize`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: JSON is registered and recommended
- **WHEN** GLib queries recommended applications for `application/json`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: JSON5 is registered and recommended
- **WHEN** GLib queries recommended applications for `application/json5`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: TOML is registered and recommended
- **WHEN** GLib queries recommended applications for `application/toml`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: YAML is registered and recommended
- **WHEN** GLib queries recommended applications for `application/yaml`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: Markdown is registered and recommended
- **WHEN** GLib queries recommended applications for `text/markdown`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the registered and recommended applications

#### Scenario: Source-language document types are not explicitly advertised
- **WHEN** verification inspects the packaged or staged LushText desktop entry after cache refresh
- **THEN** its explicit `MimeType` field does not include `text/x-csrc`, `text/x-chdr`, `text/x-python`, or `text/x-rust`

#### Scenario: Installed Flatpak keeps curated handler rows visible
- **WHEN** the installed Flatpak export is the active app-info source for `dev.cominotti.lushtext.desktop`
- **THEN** GNOME/GLib exposes LushText as a registered and recommended handler for `text/plain`, `application/x-zerosize`, `application/json`, `application/json5`, `application/toml`, `application/yaml`, and `text/markdown`

#### Scenario: Stale development entry cannot satisfy Flatpak handler acceptance
- **WHEN** a same-ID non-Flatpak development desktop entry shadows the Flatpak export
- **THEN** verification does not accept that entry as satisfying the installed Flatpak File Types requirement

### Requirement: User Defaults Are Not Mutated
Installing, building, or staging LushText SHALL register LushText as an available document handler for the curated document MIME types without changing the user's existing default application choices.

#### Scenario: Existing default remains unchanged after registration
- **WHEN** another app is the user's default handler for any curated document MIME type
- **THEN** installing or staging LushText does not replace that default with `dev.cominotti.lushtext.desktop`

#### Scenario: User can choose LushText through standard tools
- **WHEN** the user selects LushText as the default handler for `text/plain`, `application/x-zerosize`, `application/json`, `application/json5`, `application/toml`, `application/yaml`, or `text/markdown` through GNOME Settings or GLib MIME tools
- **THEN** the system records `dev.cominotti.lushtext.desktop` as the default for the selected type

### Requirement: Desktop Metadata Validation
The implementation SHALL validate the generated desktop metadata and GLib-visible MIME registration as part of the change's verification.

#### Scenario: Desktop metadata validates
- **WHEN** the generated `.desktop` file is checked with `desktop-file-validate`
- **THEN** validation succeeds without MIME-related errors

#### Scenario: GLib-visible MIME registration validates
- **WHEN** verification runs after cache refresh
- **THEN** `gio mime text/plain`, `gio mime application/x-zerosize`, `gio mime application/json`, `gio mime application/json5`, `gio mime application/toml`, `gio mime application/yaml`, and `gio mime text/markdown` each list `dev.cominotti.lushtext.desktop` as a registered and recommended application

#### Scenario: Removed source MIME registration validates
- **WHEN** verification inspects the generated and staged desktop entries after cache refresh
- **THEN** their `MimeType` fields do not include `text/x-csrc`, `text/x-chdr`, `text/x-python`, or `text/x-rust`
