# Collaborative Bookmarks & Annotations

## Status: Proposed

## Description
Per-file line bookmarks and optional inline margin annotations (short text notes
anchored to line ranges). Bookmarks persist across sessions in workspace metadata.
Annotations are stored as sidecar JSON — never touching the original file. A
non-destructive alternative to `// TODO` comments for personal notes, code review
marks, and reference points in large files.

## Current State
- No bookmarking system exists
- No annotation or note-taking capability
- GtkSourceView has built-in `GtkSourceMark` and `GtkSourceGutterRenderer` APIs that
  could support both features
- No sidecar metadata storage for files

## Motivation
Users currently hack bookmarks with `// TODO` comments, `grep`, or mental memory.
Sysadmins bookmark important stanzas in long config files. Writers leave notes-to-self
in manuscripts. Reviewers annotate a colleague's script. All of these modify the original
file, which is unacceptable for read-only files, shared files, or files under version
control where you don't want noise in your commits. Non-destructive, persistent bookmarks
and annotations fill this gap.

## Implementation Plan

### Phase 1: Line Bookmarks (model + service)
1. New `model/bookmark.rs`:
   - `Bookmark { line: u32, label: Option<String>, created_at: u64 }`
   - `FileBookmarks { path: PathBuf, bookmarks: Vec<Bookmark> }`
2. New `services/bookmark_service.rs`:
   - Storage: `$XDG_DATA_HOME/lushtext/bookmarks/<file_hash>.json`
   - `load_bookmarks(path) -> Vec<Bookmark>`
   - `save_bookmarks(path, bookmarks)`
   - `toggle_bookmark(path, line) -> bool` (returns whether added or removed)
3. Bookmark persistence uses the same `DefaultHasher` file identification as drafts

### Phase 2: Bookmark UI
1. Gutter marks: use `GtkSourceMark` (category: `"bookmark"`) + `GtkSourceMarkAttributes`
   with a bookmark icon in the gutter
2. Toggle: `Ctrl+B` on the current line adds/removes a bookmark
3. Navigation: `F2` / `Shift+F2` jumps to next/previous bookmark in the file
4. Bookmark panel (or command palette mode): list all bookmarks across all open files
   with fuzzy search on file name + bookmark label
5. Bookmark labels: optional — `Ctrl+Shift+B` prompts for a label string

### Phase 3: Annotations (model + service)
1. Extend `model/bookmark.rs` or new `model/annotation.rs`:
   - `Annotation { start_line: u32, end_line: u32, text: String, color: AnnotationColor,
     created_at: u64, updated_at: u64 }`
   - `AnnotationColor` enum: `Note` (blue), `Warning` (yellow), `Important` (red),
     `Done` (green)
2. Storage alongside bookmarks: `$XDG_DATA_HOME/lushtext/annotations/<file_hash>.json`
3. Annotations anchor to line ranges, not character offsets — simpler and more resilient
   to edits within the annotated region

### Phase 4: Annotation UI
1. Gutter indicator: colored mark in the gutter for annotated line ranges
2. Margin popover: click the gutter indicator to view/edit the annotation text
3. Create annotation: select a line range, right-click → "Add Annotation" (or
   `Ctrl+Shift+A`)
4. Inline rendering (optional, toggleable): show annotation text as a faded line below
   the annotated region (similar to CodeLens in VS Code but for user notes)
5. Annotation panel: list all annotations for the current file, click to jump

### Phase 5: Line Tracking
1. When the file is edited, bookmark and annotation line numbers must update:
   - Lines inserted above a bookmark push it down
   - Lines deleted above a bookmark pull it up
   - Deleting an annotated line range removes the annotation (with undo support)
2. Use `GtkTextBuffer::connect_insert_text` and `connect_delete_range` signals to
   track line shifts
3. Alternatively, use `GtkSourceMark` which automatically tracks position through edits
   (marks move with their line) — this is the preferred approach for bookmarks
4. Annotations (multi-line ranges) need manual tracking since `GtkSourceMark` is per-line

### Phase 6: Export / Share
1. "Export Annotations" command: generates a markdown file with annotations grouped by
   file, including line numbers and surrounding context
2. Useful for code review handoffs: annotate a colleague's code, export, send the markdown
3. Import annotations from markdown (stretch goal)

## Architecture Considerations
- `GtkSourceMark` is the natural choice for bookmarks — it's part of GtkSourceView,
  automatically tracks position through edits, and integrates with the gutter renderer.
  No custom position tracking needed.
- Annotations spanning multiple lines are harder because `GtkSourceMark` is point-based.
  Use two marks (start + end) per annotation, with the mark category encoding the
  annotation ID.
- Sidecar JSON files are essential — annotations must never modify the original file.
  This means they can go stale if the file is edited outside LushText. The line tracking
  logic only works while the file is open in the editor; on reopen, annotations are
  applied to whatever the current line numbers are.
- The `file_hash` approach (same as drafts) means renamed files keep their bookmarks.
  However, duplicated files would share bookmarks, which is incorrect. Consider using
  inode or content hash instead for more accurate identification.

## Dependencies
- `GtkSourceMark` + `GtkSourceMarkAttributes` (built into GtkSourceView)
- `GtkSourceGutterRenderer` for custom gutter rendering
- Existing `DefaultHasher` file identification pattern (from draft_service.rs)
- New GSettings key: `show-bookmarks-gutter` (b, default true)

## Risks
- Line tracking for annotations is fragile when files are edited outside LushText.
  Annotations could drift to incorrect lines. Mitigation: store a content hash of the
  annotated lines and warn the user when the content no longer matches.
- The feature could feel heavy if annotations have too much UI presence. The default
  should be minimal (small gutter marks only) with optional inline rendering as an
  explicit toggle.
- `GtkSourceMark` density: hundreds of bookmarks in one file could slow gutter rendering.
  Cap at a reasonable number (e.g., 500 per file) and test performance.
