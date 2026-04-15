## 1. Preview Context And Target Resolution

- [x] 1.1 Extend Markdown preview refresh plumbing so `LushtextMarkdownPreview` receives the active file path and current workspace roots as render context.
- [x] 1.2 Add shared preview target resolution and launch helpers for supported links and local image destinations, including explicit unsupported and unresolved outcomes.

## 2. Interactive Links And Nested Lists

- [x] 2.1 Track rendered link spans in text-buffer preview content and wire `GtkTextView` interaction controllers so supported links open externally from prose, footnotes, and callouts.
- [x] 2.2 Make list rendering depth-aware so nested ordered and unordered lists keep readable indentation and marker sequencing without regressing task-list markers.
- [x] 2.3 Extend rendered table cells so supported links keep link styling and activate through the shared preview launcher while preserving the current table layout.

## 3. Local Image Preview Blocks

- [x] 3.1 Generalize anchored preview widget cleanup from tables to reusable embedded preview blocks and render resolved local images as bounded native image widgets in document flow.
- [x] 3.2 Add explicit in-flow fallback states for unresolved local images and remote image destinations without introducing remote fetch behavior.

## 4. Regression Coverage

- [x] 4.1 Add focused unit coverage for preview target resolution, table-cell link markup/activation plumbing, and nested-list formatting state.
- [x] 4.2 Add widget coverage for clickable preview links, table-cell link behavior, rendered local images, and image fallback states.
- [x] 4.3 Run the targeted Markdown preview verification commands and confirm the new follow-up behaviors stay within the native preview path.
