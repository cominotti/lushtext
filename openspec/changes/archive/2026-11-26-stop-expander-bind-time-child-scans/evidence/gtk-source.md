# GTK Source Verification (`gtktreeexpander.c`, `gtktreelistmodel.c`)

Verified against upstream GTK sources fetched into a scratch directory outside the repository (`/tmp/gtk-src-*`, removed after inspection) for both the GNOME 50 source floor (`4.22.2`) and the GNOME 51 shipped / host toolkit (`4.24.1`, matching `pkg-config --modversion gtk4` = `4.24.1`).

## `gtk/gtktreeexpander.c`

| Fact | GTK `4.22.2` | GTK `4.24.1` |
| --- | --- | --- |
| `gtk_tree_expander_update_for_list_row` evaluates `gtk_tree_list_row_is_expandable (self->list_row) && !self->hide_expander` (`is_expandable` runs first, before checking `hide-expander`) | `:167`, `:189` | `:189` |
| `gtk_tree_expander_list_row_notify_cb` handles `"expanded"` and `"item"` explicitly and falls through to `gtk_tree_expander_update_for_list_row (self)` for all other row property notifications (including `"children"`) | `:278–311`, `:309` | `:278–311`, `:309` |
| `gtk_tree_expander_set_list_row` calls `gtk_tree_expander_update_for_list_row (self)` on every bind/rebind | `:882–904`, `:904` | `:871–899`, `:893` |
| `gtk_tree_expander_set_indent_for_depth` / `set_indent_for_icon` / `set_hide_expander` call `gtk_tree_expander_update_for_list_row (self)` | `:950`, `:993`, `:1036` | `:982`, `:1025` (plus `set_indent_for_depth`) |

## `gtk/gtktreelistmodel.c`

Line numbers in `gtktreelistmodel.c` for these functions match between `4.22.2` and `4.24.1`:

| Fact | GTK `4.22.2` & `4.24.1` |
| --- | --- |
| `tree_node_create_model` invokes `self->create_func (node->item, self->user_data)` and caches only `NULL` via `node->empty = TRUE` | `:281–291` |
| `gtk_tree_list_model_clear_node_children` disconnects `items-changed` and clears `node->model` via `g_clear_object (&node->model)` | `:399–410`, `:406` |
| `gtk_tree_list_model_expand_node` returns `0` if `node->empty` or `node->model != NULL`, else calls `tree_node_create_model` and installs the returned model via `gtk_tree_list_model_init_node` | `:491–512`, `:502`, `:507` |
| `gtk_tree_list_model_collapse_node` clears `node->model` via `gtk_tree_list_model_clear_node_children` | `:515–530`, `:525` |
| `gtk_tree_list_row_dispose` clears weak node pointer `self->node->row = NULL` (rows are cached weakly per node and recreated on later `row()` lookups) | `:1029–1039`, `:1034` |
| `gtk_tree_list_row_set_expanded` calls `expand_node` (`:1200`) or `collapse_node` (`:1209`), emits `items-changed` / `notify::n-items` on the model, then emits `notify::expanded` (`:1217`) and `notify::children` (`:1218`) | `:1178–1219` |
| `gtk_tree_list_row_is_expandable` returns `FALSE` if `node->empty`, `TRUE` if `node->model != NULL`, and otherwise calls `tree_node_create_model`, calls `g_object_unref (model)` immediately, and returns `TRUE` without caching the positive answer | `:1255–1283`, `:1265–1280` |
