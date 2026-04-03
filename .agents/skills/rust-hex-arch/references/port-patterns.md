# Port Patterns for Rust GTK4 Applications

When to use free functions, traits, or trait objects as hexagonal ports in a desktop application.

## Decision Matrix

| Situation | Pattern | Boundary enforcement | Testability |
|-----------|---------|---------------------|-------------|
| Single implementation, simple I/O | **Free function** | Module visibility (`pub`) | Pass temp paths |
| Need to mock in unit tests | **Trait parameter** (`impl Trait`) | Static dispatch | Pass mock struct |
| Need to mock in complex tests | **Trait object** (`&dyn Trait`) | Dynamic dispatch | Pass mock via `Box<dyn>` |
| Multiple implementations at runtime | **Trait object** (`Box<dyn Trait>`) | Dynamic dispatch | Same as production |
| Cross-cutting utility | **Free function in its own module** | Module path | No mocking needed |

## Pattern 1: Free Function (Default Choice)

The function signature itself is the port. The caller depends on the function's type signature, not on a trait. This is the simplest pattern and works for the majority of service functions in a desktop app.

```rust
// services/workspace_manager.rs — the function signature IS the port
pub fn load(data_dir: &Path) -> Result<WorkspacesFile> {
    let path = data_dir.join("workspaces.json");
    json_store::load(&path)
}

pub fn save(data_dir: &Path, data: &WorkspacesFile) -> Result<()> {
    let path = data_dir.join("workspaces.json");
    json_store::save(&path, data)
}
```

**Testing**: Pass a `tempfile::TempDir` path. No mocks needed — the real file system IS the test fixture.

```rust
#[test]
fn test_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let ws = WorkspacesFile { /* ... */ };
    save(dir.path(), &ws).unwrap();
    let loaded = load(dir.path()).unwrap();
    assert_eq!(loaded, ws);
}
```

**When to upgrade**: If you find yourself needing to test a function that calls `load()` without hitting the filesystem, it's time for Pattern 2.

## Pattern 2: Trait Parameter (Static Dispatch)

Extract a trait when testability demands a seam. Use `impl Trait` for static dispatch (zero runtime cost). This is the "ports crate" pattern from invowk-rust, but applied at the module level.

```rust
// services/ports.rs — or inline in the service module
pub trait WorkspaceStore {
    fn load(&self) -> Result<WorkspacesFile>;
    fn save(&self, data: &WorkspacesFile) -> Result<()>;
}

// services/workspace_manager.rs
pub fn ensure_active(store: &impl WorkspaceStore) -> Result<WorkspaceConfig> {
    let mut data = store.load()?;
    if data.active_workspace.is_none() {
        let default = WorkspaceConfig::default();
        data.active_workspace = Some(default.id.clone());
        data.workspaces.push(default.clone());
        store.save(&data)?;
        Ok(default)
    } else {
        // ...
    }
}
```

**Testing with a mock**:
```rust
#[cfg(test)]
struct MockStore {
    data: RefCell<WorkspacesFile>,
}

#[cfg(test)]
impl WorkspaceStore for MockStore {
    fn load(&self) -> Result<WorkspacesFile> {
        Ok(self.data.borrow().clone())
    }
    fn save(&self, data: &WorkspacesFile) -> Result<()> {
        *self.data.borrow_mut() = data.clone();
        Ok(())
    }
}
```

**When to upgrade to Pattern 3**: If you need to store the trait in a struct (e.g., a long-lived service that holds a reference to its dependencies), you need dynamic dispatch.

## Pattern 3: Trait Object (Dynamic Dispatch)

Use `Box<dyn Trait>` or `&dyn Trait` when the trait needs to be stored in a struct or when there are genuinely multiple runtime implementations.

```rust
pub struct SessionManager {
    store: Box<dyn SessionStore>,
}

impl SessionManager {
    pub fn new(store: Box<dyn SessionStore>) -> Self {
        Self { store }
    }

    pub fn restore(&self, workspace_id: &WorkspaceId) -> Result<SessionData> {
        self.store.load(workspace_id)
    }
}
```

This pattern is relatively rare in a desktop app with a single persistence backend. It becomes valuable when:
- Supporting multiple storage backends (e.g., local JSON + cloud sync)
- The service is a long-lived object that outlives a single function call
- Plugin architecture (extensions providing custom adapters)

## Anti-Patterns

### The Premature Port Trait

```rust
// Don't do this — wraps a single function in unnecessary ceremony
pub trait JsonStorePort {
    fn load<T: DeserializeOwned + Default>(&self, path: &Path) -> Result<T>;
    fn save<T: Serialize>(&self, path: &Path, data: &T) -> Result<()>;
}

pub struct FileJsonStore;
impl JsonStorePort for FileJsonStore { /* ... */ }
```

`json_store::load` and `json_store::save` as free functions are perfectly adequate. The trait adds abstraction tax with no benefit — there will never be a `CloudJsonStore` or `InMemoryJsonStore` implementation.

### The God Port

```rust
// Don't do this — too many responsibilities in one trait
pub trait AppServices {
    fn load_workspaces(&self) -> Result<WorkspacesFile>;
    fn save_workspaces(&self, data: &WorkspacesFile) -> Result<()>;
    fn load_session(&self, id: &WorkspaceId) -> Result<SessionData>;
    fn save_session(&self, data: &SessionData) -> Result<()>;
    fn scan_directory(&self, path: &Path) -> Vec<(PathBuf, bool)>;
}
```

If you need traits, keep them focused on one bounded context. `WorkspaceStore` and `SessionStore` are separate ports.

## Relationship to invowk-rust Architecture

The invowk-rust project uses a multi-crate hexagonal architecture:
- `invowk-domain` — pure business logic
- `invowk-ports` — trait definitions (the boundary crate)
- `invowk-application` — use cases that depend on domain + ports
- `invowk-adapters` — concrete implementations

LushText uses a lighter-weight version of the same pattern:
- `model/` ≈ domain (pure types)
- Function signatures ≈ ports (implicit contracts)
- `services/` ≈ application + adapters (combined for simplicity)
- `ui/` ≈ driving adapters

The key difference: invowk-rust is a large CLI tool with multiple I/O backends (SSH, WASM, containers, Git), so explicit port traits in a dedicated crate are justified. LushText is a single-user desktop app with one persistence mechanism (local JSON files), so implicit ports via function signatures are appropriate.

**When to consider the invowk-rust approach for LushText**: If the app grows to need multiple storage backends, cloud sync, or a plugin system, extract port traits into a separate module or crate at that point — not before.
