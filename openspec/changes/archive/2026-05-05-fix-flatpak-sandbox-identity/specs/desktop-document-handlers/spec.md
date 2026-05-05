## MODIFIED Requirements

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
