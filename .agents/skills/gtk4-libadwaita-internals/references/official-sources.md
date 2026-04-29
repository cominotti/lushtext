# Official Sources

## Table of Contents

- Allowed documentation
- Allowed source code
- Version bounds for this repo
- Repeatable source lookup workflow
- Source-file map

## Allowed Documentation

- `https://docs.gtk.org/gtk4/`
- `https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/`
- `https://docs.gtk.org/gobject/`
- `https://docs.gtk.org/gio/`
- `https://docs.gtk.org/glib/`
- `https://gnome.pages.gitlab.gnome.org/gtksourceview/gtksourceview5/`
- `https://release.gnome.org/50/developers/`

Use the `GObject`, `Gio`, and `GLib` pages only when GTK or Libadwaita docs point into them. They are part of the same official GNOME documentation surface and often define the object, signal, action, and property contracts that GTK widgets rely on.

## Allowed Source Code

- Official source tarballs from `https://download.gnome.org/sources/gtk/4.22/`
- Official source tarballs from `https://download.gnome.org/sources/libadwaita/1.9/`
- Official source tarballs from `https://download.gnome.org/sources/gtksourceview/5.20/`
- Official upstream project pages on `https://gitlab.gnome.org/GNOME/gtk/`
- Official upstream project pages on `https://gitlab.gnome.org/GNOME/libadwaita/`
- Official upstream project pages on `https://gitlab.gnome.org/GNOME/gtksourceview/`

Prefer tarballs when you need reliable local search. GitLab pages can be useful for direct browsing, but local `rg` over tarballs is more repeatable and less fragile.

## Version Bounds For This Repo

- Rust crate `gtk4 = 0.11` with feature `gnome_50` means GTK API guidance must stay inside the GTK 4.22 family.
- Rust crate `libadwaita = 0.9` with feature `v1_9` means Libadwaita guidance must stay inside the 1.9 family.
- Rust crate `sourceview5 = 0.11` with feature `v5_18` is used on the GNOME 50 runtime. GtkSourceView guidance should stay inside the 5.20 runtime family while still checking the Rust binding feature gate for APIs that were introduced after 5.18.
- Docs pages may advertise a newer library version in the header. Use `since` markers and ignore APIs newer than GTK 4.22 or Libadwaita 1.9.
- Source tarballs can be any stable micro release inside the target family, such as `gtk-4.22.3`, `libadwaita-1.9.0`, or `gtksourceview-5.20.0`.

## Repeatable Source Lookup Workflow

Fetch official tarballs into a temporary directory:

```bash
base="$(mktemp -d /tmp/gtk-upstream.XXXXXX)"
cd "$base"
curl -L -O https://download.gnome.org/sources/gtk/4.22/gtk-4.22.3.tar.xz
curl -L -O https://download.gnome.org/sources/libadwaita/1.9/libadwaita-1.9.0.tar.xz
curl -L -O https://download.gnome.org/sources/gtksourceview/5.20/gtksourceview-5.20.0.tar.xz
tar -xf gtk-4.22.3.tar.xz
tar -xf libadwaita-1.9.0.tar.xz
tar -xf gtksourceview-5.20.0.tar.xz
```

Search for a warning string:

```bash
rg -n "Trying to measure|Allocation width too small|snapshot|Can't set new parent" \
  "$base/gtk-4.22.3" "$base/libadwaita-1.9.0" "$base/gtksourceview-5.20.0"
```

Search by subsystem instead of string:

```bash
rg -n "measure|size_allocate|queue_resize|snapshot" "$base/gtk-4.22.3/gtk"
rg -n "g_warning|g_critical|measure|size_allocate" "$base/libadwaita-1.9.0/src"
rg -n "annotation|gutter|hover|completion|style_scheme" "$base/gtksourceview-5.20.0/gtksourceview"
```

Search docs pages quickly from the shell:

```bash
python - <<'PY'
import urllib.request
url = "https://docs.gtk.org/gtk4/class.Widget.html"
text = urllib.request.urlopen(url).read().decode()
for needle in ["Minimum and natural size", "Height-for-width Geometry Management"]:
    print(needle, text.find(needle))
PY
```

## Source-File Map

- `gtk/gtkwidget.c`
  Explains widget lifecycle, allocation, snapshot, parenting, buildable helper warnings, and many core invariants.
- `gtk/gtksizerequest.c`
  Explains `gtk_widget_measure()`, request-mode validation, min and natural size checks, baseline checks, and the `Trying to measure ... needs at least ...` warning.
- `gtk/gtkboxlayout.c`
  Explains how `GtkBox` derives minimum and natural sizes from child requests, spacing, expansion, and baselines.
- `gtk/gtkpaned.c`
  Explains how the handle participates in minimum and natural size, how child slots are computed, and why paned animations are sensitive to off-by-one budgets.
- `gtk/gtkrevealer.c`
  Explains transition scaling, ceil and floor behavior, and why animated revealers can feed rounded sizes back into measurement.
- `gtk/gtklistview.c`
  Explains list virtualization, model and factory contract, CSS nodes, and built-in list actions.
- `gtk/gtklistitemmanager.c`
  Explains internal list-item reuse warnings like duplicate item detection.
- `gtk/gtkwidget.c` and `gtk/gtkbuilder*`
  Explain template parsing, layout child properties, accessibility metadata parsing, and Builder failures.
- `src/adw-navigation-split-view.c`
  Explains collapsed versus uncollapsed behavior, sidebar width math, tag invariants, and navigation actions.
- `src/adw-toolbar-view.c`
  Explains top and bottom bar measurement, reveal behavior, extend-content rules, and undershoot CSS behavior.
- `src/adw-breakpoint.c`
  Explains breakpoint condition parsing, setter application, object lookup, and property validation warnings.
- `src/adw-view-stack.c`
  Explains page naming invariants and page lookup failures.
- `gtksourceview/gtksourceannotation.c`, `gtksourceannotationprovider.c`, and `gtksourceannotations.c`
  Explain native annotation creation, provider ownership, update notifications, and how annotations attach to a `GtkSourceView`.
- `gtksourceview/gtksourceview.c`
  Explains editor widget behavior, gutters, marks, hover providers, annotations, and source-view-specific snapshot and input handling.

## Use This File

- Start here when you need the exact upstream file to inspect.
- Use the tarball workflow when a warning string is known.
- Use the source-file map when the symptom is behavioral rather than textual.
