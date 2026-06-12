// SPDX-License-Identifier: GPL-3.0-or-later

//! Host capability and isolated runtime setup for live visual proof.
//!
//! Live visual proof depends on session services that vary by developer
//! machine. Keeping probes and runtime directory setup separate from scenario
//! execution lets `cargo gtk-proof run` explain unsupported hosts without
//! pretending that skipped runs verified visual invariants.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{artifacts, model};

/// Result of probing all host support required by the live runner.
#[derive(Debug)]
pub(crate) struct HostProbeReport {
    capabilities: Vec<CapabilityReport>,
    missing_capabilities: Vec<String>,
}

impl HostProbeReport {
    /// Return a bounded list of missing required capabilities.
    pub(crate) fn missing_capabilities(&self) -> &[String] {
        &self.missing_capabilities
    }

    fn status(&self) -> &'static str {
        if self.missing_capabilities.is_empty() {
            "ready"
        } else {
            "unsupported-host"
        }
    }
}

/// Isolated directories used by one visual proof run.
#[derive(Debug)]
pub(crate) struct RuntimeLayout {
    root: PathBuf,
    runtime_dir: PathBuf,
    data_dir: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
}

impl RuntimeLayout {
    /// Create runtime, data, config, and cache directories under the artifact root.
    pub(crate) fn prepare(artifact_dir: &Path) -> Result<Self, String> {
        let root = artifact_dir.join("runtime");
        let runtime_dir = runtime_dir_for_artifact(artifact_dir, &root)?;
        let data_dir = root.join("data");
        let config_dir = root.join("config");
        let cache_dir = root.join("cache");
        for dir in [&runtime_dir, &data_dir, &config_dir, &cache_dir] {
            fs::create_dir_all(dir)
                .map_err(|error| format!("cannot create runtime dir {}: {error}", dir.display()))?;
        }
        restrict_runtime_dir(&runtime_dir)?;
        Ok(Self {
            root,
            runtime_dir,
            data_dir,
            config_dir,
            cache_dir,
        })
    }

    fn report(&self) -> RuntimeLayoutReport {
        RuntimeLayoutReport {
            root: artifacts::safe_display_path(&self.root),
            xdg_runtime_dir: artifacts::safe_display_path(&self.runtime_dir),
            xdg_data_home: artifacts::safe_display_path(&self.data_dir),
            xdg_config_home: artifacts::safe_display_path(&self.config_dir),
            xdg_cache_home: artifacts::safe_display_path(&self.cache_dir),
            session_bus: "dbus-run-session".to_string(),
            session_command: Self::session_command_report(),
            environment: self.environment_report(),
        }
    }

    /// Return process environment overrides for the future live-runner child.
    pub(crate) fn process_environment(&self) -> Vec<(String, String)> {
        [
            ("GSETTINGS_BACKEND", "keyfile".to_string()),
            (
                "GSETTINGS_SCHEMA_DIR",
                gsettings_schema_dir().to_string_lossy().into_owned(),
            ),
            (
                "LUSHTEXT_DATA_DIR",
                self.data_dir.to_string_lossy().into_owned(),
            ),
            (
                "XDG_CACHE_HOME",
                self.cache_dir.to_string_lossy().into_owned(),
            ),
            (
                "XDG_CONFIG_HOME",
                self.config_dir.to_string_lossy().into_owned(),
            ),
            (
                "XDG_DATA_HOME",
                self.data_dir.to_string_lossy().into_owned(),
            ),
            (
                "XDG_RUNTIME_DIR",
                self.runtime_dir.to_string_lossy().into_owned(),
            ),
            ("NO_AT_BRIDGE", "1".to_string()),
            ("GSK_RENDERER", "cairo".to_string()),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
    }

    /// Return the private XDG runtime directory used for session sockets.
    pub(crate) fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// Remove volatile session sockets after the supervised child exits.
    pub(crate) fn cleanup_runtime_dir(&self) -> String {
        match fs::remove_dir_all(&self.runtime_dir) {
            Ok(()) => "removed".to_string(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "removed".to_string(),
            Err(error) => format!("remove_failed: {error}"),
        }
    }

    /// Return the outer session command prefix used before compositor launch.
    pub(crate) fn session_command(child: &[String]) -> Vec<String> {
        let mut command = vec!["dbus-run-session".to_string(), "--".to_string()];
        command.extend_from_slice(child);
        command
    }

    fn session_command_report() -> Vec<String> {
        Self::session_command(&["cargo-gtk-proof-live-child".to_string()])
    }

    fn environment_report(&self) -> Vec<RuntimeEnvVarReport> {
        self.process_environment()
            .into_iter()
            .map(|(key, value)| RuntimeEnvVarReport {
                key,
                value: report_env_value(&value),
            })
            .collect()
    }
}

/// Probe live-runner host capabilities without launching the app.
pub(crate) fn probe_host(binary: &Path) -> HostProbeReport {
    let mut capabilities = Vec::new();
    for command in [
        "dbus-run-session",
        "gdbus",
        "gsettings",
        "gst-launch-1.0",
        "mutter",
        "pipewire",
        "pw-dump",
        "wireplumber",
    ] {
        capabilities.push(probe_command(command));
    }
    capabilities.push(probe_file("/usr/bin/python3", "python-oracle", true));
    capabilities.push(probe_binary(binary));
    capabilities.push(CapabilityReport {
        name: "rust-png-decoder".to_string(),
        kind: "built-in".to_string(),
        required: true,
        available: true,
        path: None,
        detail: "built into cargo-gtk-proof".to_string(),
    });
    let missing_capabilities = capabilities
        .iter()
        .filter(|capability| capability.required && !capability.available)
        .map(|capability| capability.detail.clone())
        .collect();
    HostProbeReport {
        capabilities,
        missing_capabilities,
    }
}

/// Write the run-level environment report.
pub(crate) fn write_environment_report(
    artifact_dir: &Path,
    probe: &HostProbeReport,
    runtime: &RuntimeLayout,
) -> Result<PathBuf, String> {
    let report = EnvironmentReport {
        schema_version: model::SUPPORTED_SCHEMA_VERSION,
        status: probe.status(),
        host_capabilities: probe.capabilities.clone(),
        missing_capabilities: probe.missing_capabilities.clone(),
        runtime: runtime.report(),
    };
    artifacts::write_artifact(
        &artifact_dir.join("environment-report.json"),
        artifacts::ProofArtifactKind::EnvironmentReport,
        &report,
    )
}

#[derive(Clone, Debug, Serialize)]
struct CapabilityReport {
    name: String,
    kind: String,
    required: bool,
    available: bool,
    path: Option<String>,
    detail: String,
}

#[derive(Debug, Serialize)]
struct RuntimeLayoutReport {
    root: String,
    xdg_runtime_dir: String,
    xdg_data_home: String,
    xdg_config_home: String,
    xdg_cache_home: String,
    session_bus: String,
    session_command: Vec<String>,
    environment: Vec<RuntimeEnvVarReport>,
}

#[derive(Clone, Debug, Serialize)]
struct RuntimeEnvVarReport {
    key: String,
    value: String,
}

#[derive(Debug, Serialize)]
struct EnvironmentReport {
    schema_version: u64,
    status: &'static str,
    host_capabilities: Vec<CapabilityReport>,
    missing_capabilities: Vec<String>,
    runtime: RuntimeLayoutReport,
}

fn probe_command(command: &str) -> CapabilityReport {
    match find_command(command) {
        Some(path) => CapabilityReport {
            name: command.to_string(),
            kind: "command".to_string(),
            required: true,
            available: true,
            path: Some(path.to_string_lossy().into_owned()),
            detail: format!("found required command: {command}"),
        },
        None => CapabilityReport {
            name: command.to_string(),
            kind: "command".to_string(),
            required: true,
            available: false,
            path: None,
            detail: format!("missing required command: {command}"),
        },
    }
}

fn probe_file(path: impl AsRef<Path>, name: &str, required: bool) -> CapabilityReport {
    let path = path.as_ref();
    let available = path.is_file();
    CapabilityReport {
        name: name.to_string(),
        kind: "file".to_string(),
        required,
        available,
        path: Some(path.to_string_lossy().into_owned()),
        detail: if available {
            format!("found {}", path.display())
        } else {
            format!("missing {}", path.display())
        },
    }
}

fn probe_binary(binary: &Path) -> CapabilityReport {
    let available = binary.is_file() && is_executable(binary);
    CapabilityReport {
        name: "lushtext-binary".to_string(),
        kind: "binary".to_string(),
        required: true,
        available,
        path: Some(binary.to_string_lossy().into_owned()),
        detail: if available {
            format!("LushText debug binary is executable: {}", binary.display())
        } else {
            format!(
                "LushText debug binary is missing or not executable: {}",
                binary.display()
            )
        },
    }
}

fn find_command(command: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(command))
        .find(|candidate| candidate.is_file() && is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn restrict_runtime_dir(runtime_dir: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(runtime_dir)
            .map_err(|error| format!("cannot inspect {}: {error}", runtime_dir.display()))?
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(runtime_dir, permissions)
            .map_err(|error| format!("cannot chmod {}: {error}", runtime_dir.display()))?;
    }
    Ok(())
}

fn runtime_dir_for_artifact(artifact_dir: &Path, root: &Path) -> Result<PathBuf, String> {
    let artifact_local = root.join("xdg-runtime");
    if pipewire_manager_socket_path_len(&artifact_local) < 108 {
        return Ok(artifact_local);
    }

    let mut hasher = DefaultHasher::new();
    absolute_for_hash(artifact_dir)?.hash(&mut hasher);
    let hash = hasher.finish();
    Ok(std::env::temp_dir().join(format!("lt-proof-{}-{hash:016x}", std::process::id())))
}

fn absolute_for_hash(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .map_err(|error| format!("cannot read current dir for runtime hash: {error}"))?
            .join(path))
    }
}

fn pipewire_manager_socket_path_len(runtime_dir: &Path) -> usize {
    runtime_dir
        .join("pipewire-0-manager")
        .to_string_lossy()
        .len()
        + 1
}

fn gsettings_schema_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data")
}

fn report_env_value(value: &str) -> String {
    let path = Path::new(value);
    if path.is_absolute() || value.contains('/') {
        artifacts::safe_display_path(path)
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn probe_reports_missing_or_non_executable_binary() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let binary = tempdir.path().join("lushtext");
        fs::write(&binary, b"not executable").expect("binary fixture");

        let probe = probe_host(&binary);

        assert!(
            probe
                .missing_capabilities()
                .iter()
                .any(|detail| detail.contains("not executable"))
        );
    }

    #[test]
    fn runtime_layout_creates_isolated_xdg_dirs() {
        let tempdir = tempfile::tempdir().expect("tempdir");

        let layout = RuntimeLayout::prepare(tempdir.path()).expect("runtime layout");

        for dir in [
            &layout.runtime_dir,
            &layout.data_dir,
            &layout.config_dir,
            &layout.cache_dir,
        ] {
            assert!(dir.is_dir(), "{} exists", dir.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&layout.runtime_dir)
                .expect("runtime metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o700);
        }
    }

    #[test]
    fn runtime_layout_uses_short_runtime_dir_for_pipewire_socket_limit() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let long_case_dir = tempdir
            .path()
            .join("build")
            .join("smoke")
            .join("visual-geometry")
            .join("minimap-sidebar-top--maximized-like--force-light--wrap-true--show");

        let layout = RuntimeLayout::prepare(&long_case_dir).expect("runtime layout");

        assert!(
            pipewire_manager_socket_path_len(&layout.runtime_dir) < 108,
            "PipeWire socket path must fit sockaddr_un: {}",
            layout.runtime_dir.display()
        );
        assert!(
            !layout.runtime_dir.starts_with(&long_case_dir),
            "long artifact paths should use a short temp runtime dir"
        );
        assert!(layout.data_dir.starts_with(&long_case_dir));
    }

    #[test]
    fn runtime_layout_exports_isolated_session_environment() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let layout = RuntimeLayout::prepare(tempdir.path()).expect("runtime layout");
        let env = layout.process_environment();

        for (key, expected) in [
            ("GSETTINGS_BACKEND", "keyfile"),
            ("NO_AT_BRIDGE", "1"),
            ("GSK_RENDERER", "cairo"),
        ] {
            assert!(
                env.iter()
                    .any(|(actual_key, value)| actual_key == key && value == expected),
                "missing {key}={expected}"
            );
        }
        for (key, path) in [
            ("XDG_RUNTIME_DIR", &layout.runtime_dir),
            ("XDG_DATA_HOME", &layout.data_dir),
            ("XDG_CONFIG_HOME", &layout.config_dir),
            ("XDG_CACHE_HOME", &layout.cache_dir),
            ("LUSHTEXT_DATA_DIR", &layout.data_dir),
        ] {
            let expected_path = path.to_string_lossy();
            assert!(
                env.iter().any(|(actual_key, value)| actual_key == key
                    && value == expected_path.as_ref()),
                "missing isolated path for {key}"
            );
        }
        assert!(
            env.iter()
                .any(|(key, value)| key == "GSETTINGS_SCHEMA_DIR" && value.ends_with("/data")),
            "missing schema dir"
        );
        assert_eq!(
            RuntimeLayout::session_command(&["child".to_string(), "--flag".to_string()]),
            ["dbus-run-session", "--", "child", "--flag"]
                .map(String::from)
                .to_vec()
        );
    }

    #[test]
    fn environment_report_is_schema_valid() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let layout = RuntimeLayout::prepare(tempdir.path()).expect("runtime layout");
        let probe = probe_host(Path::new("/definitely/missing/lushtext"));

        let path =
            write_environment_report(tempdir.path(), &probe, &layout).expect("environment report");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("report text")).expect("json");

        assert_eq!(value["status"], "unsupported-host");
        assert_eq!(value["runtime"]["session_command"][0], "dbus-run-session");
        assert!(
            value["runtime"]["environment"]
                .as_array()
                .expect("runtime env")
                .iter()
                .any(|entry| entry["key"] == "XDG_RUNTIME_DIR")
        );
        model::validate_document(&value).expect("schema-valid environment report");
    }
}
