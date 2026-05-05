# desktop-document-handlers Specification

## Purpose
Define the desktop metadata contract that lets GNOME Settings and GLib expose LushText as an available handler for plain text, Markdown, and empty documents without changing the user's default application choices.

## Requirements

### Requirement: Advertised Document MIME Types
LushText SHALL advertise support for the document MIME types needed for GNOME's standard file association UI: `text/plain`, `text/markdown`, and `application/x-zerosize`.

#### Scenario: Shipped desktop entry declares required document handlers
- **WHEN** the packaged LushText desktop entry is generated
- **THEN** its `MimeType` field includes `text/plain`, `text/markdown`, and `application/x-zerosize`

#### Scenario: Development desktop entry mirrors required document handlers
- **WHEN** the local development desktop entry is staged for GNOME Shell and Settings integration
- **THEN** its `MimeType` field includes the same required document MIME types as the packaged desktop entry

### Requirement: GNOME Files And Links Visibility
LushText SHALL appear in GNOME/GLib application association data as a recommended handler for Plain Text, Markdown, and Empty document file types after its desktop entry is installed or staged and the relevant desktop MIME cache is refreshed. When the Flatpak is installed, this handler visibility SHALL be provided through the Flatpak-backed `dev.cominotti.lushtext.desktop` app info rather than through a stale same-ID non-Flatpak development desktop entry.

#### Scenario: Plain text is recommended
- **WHEN** GLib queries recommended applications for `text/plain`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the recommended applications

#### Scenario: Markdown is recommended
- **WHEN** GLib queries recommended applications for `text/markdown`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the recommended applications

#### Scenario: Empty document is recommended
- **WHEN** GLib queries recommended applications for `application/x-zerosize`
- **THEN** `dev.cominotti.lushtext.desktop` is included in the recommended applications

#### Scenario: Installed Flatpak keeps handler rows visible
- **WHEN** the installed Flatpak export is the active app-info source for `dev.cominotti.lushtext.desktop`
- **THEN** GNOME/GLib still exposes LushText as a recommended handler for `text/plain`, `text/markdown`, and `application/x-zerosize`

#### Scenario: Stale development entry cannot satisfy Flatpak handler acceptance
- **WHEN** a same-ID non-Flatpak development desktop entry shadows the Flatpak export
- **THEN** verification does not accept that entry as satisfying the installed Flatpak Files & Links requirement

### Requirement: User Defaults Are Not Mutated
Installing, building, or staging LushText SHALL register LushText as an available document handler without changing the user's existing default application choices.

#### Scenario: Existing default remains unchanged after registration
- **WHEN** another app is the user's default handler for `application/x-zerosize`
- **THEN** installing or staging LushText does not replace that default with `dev.cominotti.lushtext.desktop`

#### Scenario: User can choose LushText through standard tools
- **WHEN** the user selects LushText as the default handler for `text/plain`, `text/markdown`, or `application/x-zerosize` through GNOME Settings or GLib MIME tools
- **THEN** the system records `dev.cominotti.lushtext.desktop` as the default for the selected type

### Requirement: Desktop Metadata Validation
The implementation SHALL validate the generated desktop metadata and GLib-visible MIME registration as part of the change's verification.

#### Scenario: Desktop metadata validates
- **WHEN** the generated `.desktop` file is checked with `desktop-file-validate`
- **THEN** validation succeeds without MIME-related errors

#### Scenario: GLib-visible MIME registration validates
- **WHEN** verification runs after cache refresh
- **THEN** `gio mime text/plain`, `gio mime text/markdown`, and `gio mime application/x-zerosize` each list `dev.cominotti.lushtext.desktop` as a registered and recommended application
