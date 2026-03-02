use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
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
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson {
    pub dependencies: Option<HashMap<String, String>>,
    pub dev_dependencies: Option<HashMap<String, String>>,
    pub peer_dependencies: Option<HashMap<String, String>>,
    pub optional_dependencies: Option<HashMap<String, String>>,
}

impl PackageJson {
    pub fn from_path(project_path: &Path) -> Result<Self, PackageJsonError> {
        let path = project_path.join("package.json");
        let content = std::fs::read_to_string(&path).map_err(|e| PackageJsonError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        serde_json::from_str(&content).map_err(|e| PackageJsonError::Parse {
            path: path.display().to_string(),
            source: e,
        })
    }

    /// Returns the declared version range if the package is a direct dependency,
    /// or None if it is not declared in any bucket.
    pub fn get_version(&self, name: &str) -> Option<Version> {
        [
            &self.dependencies,
            &self.dev_dependencies,
            &self.peer_dependencies,
            &self.optional_dependencies,
        ]
        .iter()
        .find_map(|bucket| {
            bucket
                .as_ref()
                .and_then(|m| m.get(name))
                .map(|v| Version::from(v.as_str()))
        })
    }

    pub fn set_version(&mut self, name: &str, version: &str) -> bool {
        let buckets = [
            &mut self.dependencies,
            &mut self.dev_dependencies,
            &mut self.peer_dependencies,
            &mut self.optional_dependencies,
        ];

        for bucket in buckets {
            if let Some(map) = bucket {
                if map.contains_key(name) {
                    map.insert(name.to_string(), version.to_string());
                    return true;
                }
            }
        }

        if let Some(ref mut deps) = self.dependencies {
            deps.insert(name.to_string(), version.to_string());
            return true;
        }

        self.dependencies = Some(HashMap::from([(name.to_string(), version.to_string())]));
        true
    }

    pub fn write(&self, project_path: &Path) -> Result<(), PackageJsonError> {
        let path = project_path.join("package.json");
        let content = serde_json::to_string_pretty(self).map_err(|e| PackageJsonError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;
        std::fs::write(&path, content).map_err(|e| PackageJsonError::Write {
            path: path.display().to_string(),
            source: e,
        })
    }
}
