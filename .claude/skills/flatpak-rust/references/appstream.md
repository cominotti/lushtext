# AppStream Metainfo for LushText

Complete template and guidelines for the AppStream metainfo file.

## Template: `data/dev.cominotti.lushtext.metainfo.xml.in`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>dev.cominotti.lushtext</id>

  <name>LushText</name>
  <summary>A minimalist text editor with workspace support</summary>

  <developer id="dev.cominotti">
    <name>Danilo Cominotti</name>
  </developer>

  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-or-later</project_license>

  <description>
    <p>
      LushText is a fast, minimalist text editor for GNOME. It features a
      left-side file tree, workspace support for organizing projects, and
      syntax highlighting for common file types.
    </p>
    <p>Features:</p>
    <ul>
      <li>Always-visible file tree sidebar</li>
      <li>Workspace support with multiple root directories</li>
      <li>Syntax highlighting via GtkSourceView</li>
      <li>Tab-based editing with session persistence</li>
      <li>Automatic dark mode support</li>
      <li>Find and replace</li>
    </ul>
  </description>

  <launchable type="desktop-id">dev.cominotti.lushtext.desktop</launchable>

  <url type="homepage">https://github.com/cominotti/lushtext</url>
  <url type="bugtracker">https://github.com/cominotti/lushtext/issues</url>
  <url type="vcs-browser">https://github.com/cominotti/lushtext</url>

  <branding>
    <color type="primary" scheme_preference="light">#62a0ea</color>
    <color type="primary" scheme_preference="dark">#1a5fb4</color>
  </branding>

  <content_rating type="oars-1.1"/>

  <supports>
    <control>pointing</control>
    <control>keyboard</control>
  </supports>

  <requires>
    <display_length compare="ge">360</display_length>
  </requires>

  <screenshots>
    <screenshot type="default">
      <caption>Editing a Rust file with the file tree sidebar</caption>
      <image>https://raw.githubusercontent.com/cominotti/lushtext/main/data/screenshots/editor.png</image>
    </screenshot>
    <screenshot>
      <caption>Multiple workspaces with dark mode</caption>
      <image>https://raw.githubusercontent.com/cominotti/lushtext/main/data/screenshots/dark-mode.png</image>
    </screenshot>
  </screenshots>

  <releases>
    <release version="0.1.0" date="2026-01-01">
      <description>
        <p>Initial release of LushText.</p>
      </description>
    </release>
  </releases>

  <translation type="gettext">lushtext</translation>

  <custom>
    <value key="GnomeSoftware::key-colors">[(98, 160, 234)]</value>
  </custom>
</component>
```

## Validation

```bash
# Basic validation
appstreamcli validate data/dev.cominotti.lushtext.metainfo.xml.in

# Strict validation (what Flathub uses)
appstreamcli validate --explain --pedantic data/dev.cominotti.lushtext.metainfo.xml.in
```

## Required Elements Checklist

| Element | Status | Notes |
|---------|--------|-------|
| `<id>` | Required | Must match app ID exactly |
| `<name>` | Required | Display name in software center |
| `<summary>` | Required | One-line description, <35 chars recommended |
| `<description>` | Required | At least one `<p>` paragraph |
| `<developer>` | Required | Developer name and optional ID |
| `<launchable>` | Required | Must match desktop file name |
| `<url type="homepage">` | Required | Project homepage |
| `<content_rating>` | Required | OARS 1.1 content rating |
| `<releases>` | Required | At least one `<release>` |
| `<screenshots>` | Required | At least one with `<caption>` |
| `<metadata_license>` | Required | License for the metainfo file itself (CC0-1.0 is standard) |
| `<project_license>` | Required | The app's license (GPL-3.0-or-later) |

## Screenshot Guidelines

- **Resolution**: 1602x900px (16:9) is ideal for Flathub/GNOME Software
- **Format**: PNG recommended (WebP also accepted)
- **Content**: Show the app in realistic use — actual text being edited, not lorem ipsum
- **Hosting**: Use raw.githubusercontent.com or another permanent URL
- **Dark mode**: Include at least one light and one dark screenshot
- **File tree**: Since the sidebar is a differentiating feature, show it prominently

## Branding Colors

The `<branding>` element defines accent colors for GNOME Software 45+:

```xml
<branding>
  <color type="primary" scheme_preference="light">#62a0ea</color>
  <color type="primary" scheme_preference="dark">#1a5fb4</color>
</branding>
```

These colors are used in the app's listing banner. Choose colors that complement the app's icon and visual identity. The GNOME HIG recommends using Adwaita named colors.
