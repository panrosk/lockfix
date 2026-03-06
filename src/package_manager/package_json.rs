use std::path::Path;

use serde_json::Value;
use thiserror::Error;

use super::Version;

#[derive(Debug, Error)]
pub enum PackageJsonError {
    #[error("failed to read package.json at '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse package.json at '{path}': {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to write package.json at '{path}': {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("package.json validation failed at '{path}': {reason}")]
    Validation { path: String, reason: String },
}

/// A lossless wrapper around a `package.json` file.
///
/// All fields are preserved verbatim by keeping the raw JSON map as the backing
/// store. Typed accessors read/mutate the map directly, so no field is ever
/// silently dropped during a roundtrip (the previous `#[serde(flatten)]`
/// approach had a known serde bug that could lose unknown keys).
#[derive(Debug)]
pub struct PackageJson {
    /// The raw parsed JSON — always an Object at the top level.
    inner: serde_json::Map<String, Value>,
    /// Number of top-level keys present when the file was first loaded.
    /// Used by `write()` to guard against accidental data loss.
    original_key_count: usize,
}

const BUCKETS: [&str; 4] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

impl PackageJson {
    pub fn from_path(project_path: &Path) -> Result<Self, PackageJsonError> {
        let path = project_path.join("package.json");
        let content = std::fs::read_to_string(&path).map_err(|e| PackageJsonError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        let value: Value =
            serde_json::from_str(&content).map_err(|e| PackageJsonError::Parse {
                path: path.display().to_string(),
                source: e,
            })?;
        let inner = match value {
            Value::Object(map) => map,
            _ => {
                return Err(PackageJsonError::Parse {
                    path: path.display().to_string(),
                    source: serde_json::from_str::<Value>("{invalid}").unwrap_err(),
                })
            }
        };
        let original_key_count = inner.len();
        Ok(Self {
            inner,
            original_key_count,
        })
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn dep_map(&self, key: &str) -> Option<&serde_json::Map<String, Value>> {
        self.inner.get(key).and_then(|v| v.as_object())
    }

    fn dep_map_mut(&mut self, key: &str) -> Option<&mut serde_json::Map<String, Value>> {
        self.inner.get_mut(key).and_then(|v| v.as_object_mut())
    }

    // ── public API ───────────────────────────────────────────────────────────

    /// Returns the declared version range if the package is a direct dependency,
    /// or None if it is not declared in any bucket.
    pub fn get_version(&self, name: &str) -> Option<Version> {
        for bucket in BUCKETS {
            if let Some(v) = self.dep_map(bucket).and_then(|m| m.get(name)) {
                if let Some(s) = v.as_str() {
                    return Some(Version::from(s));
                }
            }
        }
        None
    }

    /// Returns the dependency type for a package based on which bucket it's in.
    /// Returns None if the package is not a direct dependency.
    pub fn get_dependency_type(&self, name: &str) -> Option<crate::config::DependencyType> {
        use crate::config::DependencyType;
        let mapping = [
            ("dependencies", DependencyType::Dependency),
            ("devDependencies", DependencyType::DevDependency),
            ("peerDependencies", DependencyType::PeerDependency),
            ("optionalDependencies", DependencyType::OptionalDependency),
        ];
        for (key, dep_type) in mapping {
            if self.dep_map(key).is_some_and(|m| m.contains_key(name)) {
                return Some(dep_type);
            }
        }
        None
    }

    pub fn set_version(&mut self, name: &str, version: &str) -> bool {
        // Update in whichever bucket already contains the package.
        for bucket in BUCKETS {
            if let Some(map) = self.dep_map_mut(bucket) {
                if map.contains_key(name) {
                    map.insert(name.to_string(), Value::String(version.to_string()));
                    return true;
                }
            }
        }
        // Fall back: insert into dependencies, creating the object if missing.
        let deps = self
            .inner
            .entry("dependencies")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(map) = deps {
            map.insert(name.to_string(), Value::String(version.to_string()));
        }
        true
    }

    /// Validates that no top-level keys have been lost since the file was loaded.
    fn validate(&self, path: &str) -> Result<(), PackageJsonError> {
        if self.inner.len() < self.original_key_count {
            return Err(PackageJsonError::Validation {
                path: path.to_string(),
                reason: format!(
                    "top-level key count dropped from {} to {} — refusing to write to avoid data loss",
                    self.original_key_count,
                    self.inner.len()
                ),
            });
        }
        Ok(())
    }

    pub fn write(&self, project_path: &Path) -> Result<(), PackageJsonError> {
        let path = project_path.join("package.json");
        let path_str = path.display().to_string();

        self.validate(&path_str)?;

        let content =
            serde_json::to_string_pretty(&Value::Object(self.inner.clone())).map_err(|e| {
                PackageJsonError::Parse {
                    path: path_str.clone(),
                    source: e,
                }
            })?;
        std::fs::write(&path, content).map_err(|e| PackageJsonError::Write {
            path: path_str,
            source: e,
        })
    }
}
