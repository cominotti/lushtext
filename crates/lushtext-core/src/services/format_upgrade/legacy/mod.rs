// SPDX-License-Identifier: GPL-3.0-or-later

//! Legacy converter registry for app-owned metadata formats.
//!
//! Future old-version payload structs and conversion functions belong in this
//! module tree, never in ordinary runtime readers. The current public baseline
//! is v1, so the production registry is intentionally empty until a later
//! release introduces v2 or newer metadata.

use std::collections::HashMap;
#[cfg(feature = "test-utils")]
use std::sync::Mutex;

use anyhow::{Context, Result, bail};

#[cfg(feature = "test-utils")]
use crate::services::json_format::KIND_SESSION;
use crate::services::json_format::SUPPORTED_JSON_VERSION;

/// Manual-test switch that registers a synthetic v0 session converter.
///
/// This is compiled only with `test-utils`, so release builds do not learn any
/// fake old format while `make run-format-upgrade-older-manual-test` can still
/// exercise the startup Convert path in a real GUI session.
#[cfg(feature = "test-utils")]
const MANUAL_SESSION_V0_FIXTURE_ENV: &str = "LUSHTEXT_FORMAT_UPGRADE_MANUAL_SESSION_V0";

/// Function pointer that converts one recognized older envelope to latest bytes.
pub type ConverterFn = fn(&[u8]) -> Result<Vec<u8>>;

#[derive(Clone, Copy)]
struct ConverterEntry {
    to_version: u32,
    convert: ConverterFn,
}

/// Converter lookup table used by planning and apply commands.
#[derive(Clone, Default)]
pub struct ConverterRegistry {
    converters: HashMap<(&'static str, u32), ConverterEntry>,
}

#[cfg(feature = "test-utils")]
static TEST_PRODUCTION_REGISTRY: Mutex<Option<ConverterRegistry>> = Mutex::new(None);

/// Scoped override for widget tests that need the production scan/apply path to
/// see a synthetic legacy converter.
#[cfg(feature = "test-utils")]
pub struct ProductionRegistryOverride {
    previous: Option<ConverterRegistry>,
}

#[cfg(feature = "test-utils")]
impl Drop for ProductionRegistryOverride {
    fn drop(&mut self) {
        let mut guard = TEST_PRODUCTION_REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = self.previous.take();
    }
}

impl ConverterRegistry {
    /// Return the production converter registry.
    ///
    /// It is empty while v1 remains the latest supported format. Future format
    /// bumps add one converter per supported version step here.
    #[must_use]
    pub fn production() -> Self {
        #[cfg(feature = "test-utils")]
        {
            if let Some(registry) = TEST_PRODUCTION_REGISTRY
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
            {
                return registry;
            }
        }
        let registry = Self::default();
        #[cfg(feature = "test-utils")]
        {
            if std::env::var_os(MANUAL_SESSION_V0_FIXTURE_ENV).is_some() {
                return registry.with_converter(
                    KIND_SESSION,
                    0,
                    SUPPORTED_JSON_VERSION,
                    convert_manual_session_v0_fixture_to_v1,
                );
            }
        }
        registry
    }

    /// Temporarily replace the production registry for end-to-end widget tests.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn override_production_for_test(registry: Self) -> ProductionRegistryOverride {
        let previous = TEST_PRODUCTION_REGISTRY
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .replace(registry);
        ProductionRegistryOverride { previous }
    }

    /// Register a converter for tests or future version-step modules.
    #[must_use]
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_converter(
        mut self,
        kind: &'static str,
        from_version: u32,
        to_version: u32,
        convert: ConverterFn,
    ) -> Self {
        self.converters.insert(
            (kind, from_version),
            ConverterEntry {
                to_version,
                convert,
            },
        );
        self
    }

    /// Return the target version when a converter exists for this exact step.
    #[must_use]
    pub fn target_version(&self, kind: &'static str, from_version: u32) -> Option<u32> {
        self.converters
            .get(&(kind, from_version))
            .map(|entry| entry.to_version)
    }

    /// Convert bytes through the registered converter for this exact step.
    ///
    /// # Errors
    ///
    /// Returns an error if no converter exists, if the converter fails, or if
    /// the converter does not produce the current latest version.
    pub fn convert(&self, kind: &'static str, from_version: u32, bytes: &[u8]) -> Result<Vec<u8>> {
        let Some(entry) = self.converters.get(&(kind, from_version)) else {
            bail!("no converter registered for {kind} v{from_version}");
        };
        if entry.to_version != SUPPORTED_JSON_VERSION {
            bail!(
                "converter for {kind} v{from_version} targets v{}, expected v{}",
                entry.to_version,
                SUPPORTED_JSON_VERSION
            );
        }
        (entry.convert)(bytes).with_context(|| format!("failed to convert {kind} v{from_version}"))
    }
}

/// Wrap the manual v0 session fixture payload in the current v1 envelope.
///
/// The fixture intentionally preserves only the generic `data` value; real
/// historical converters should live beside this module with typed old-format
/// structs and release-specific tests.
#[cfg(feature = "test-utils")]
fn convert_manual_session_v0_fixture_to_v1(bytes: &[u8]) -> Result<Vec<u8>> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).context("manual session v0 fixture is not valid JSON")?;
    let data = value
        .get("data")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"tabs": [], "active_tab_index": null}));
    Ok(serde_json::to_vec_pretty(&serde_json::json!({
        "kind": KIND_SESSION,
        "version": SUPPORTED_JSON_VERSION,
        "data": data,
    }))?)
}
