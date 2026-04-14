## 1. Overscroll alignment

- [x] 1.1 Add GNOME-style dynamic editor bottom overscroll so mapped editor pages replace the small fixed EOF tail with viewport-scaled blank space after the last line.
- [x] 1.2 Keep the minimap geometry aligned with the editor overscroll so the map inherits the extended tail room instead of collapsing early near EOF.

## 2. Minimap interaction parity

- [x] 2.1 Remove the editor-page minimap click and drag gesture overrides that remap pointer positions through custom line math instead of `GtkSourceMap`.
- [x] 2.2 Keep minimap interaction aligned with the native map focus contract so the editor remains the primary editing surface without forcing per-gesture focus handoff.

## 3. Regression coverage

- [x] 3.1 Add focused tests that the editor bottom margin grows from the visible rect after allocation and that the minimap receives the resulting tail geometry.
- [x] 3.2 Replace the old pointer-to-line helper coverage with tests that guard the new native-navigation integration boundary.
- [x] 3.3 Add widget coverage that the constructed minimap source map does not carry LushText-owned click or drag gesture controllers on top of the native widget configuration.

## 4. Verification

- [x] 4.1 Run the focused editor-page and minimap verification for dynamic overscroll plus the existing minimap interaction regression guards.
