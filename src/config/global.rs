use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GlobalConfigError {
    #[error("could not determine config directory — $HOME may not be set")]
    NoConfigDir,

    #[error("failed to read config file at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("failed to write config file at {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("invalid config file at {path}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),

    #[error("editor '{0}' already exists")]
    EditorAlreadyExists(String),

    #[error("editor '{0}' not found")]
    EditorNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub copy: Vec<String>,
    #[serde(default)]
    pub link: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    #[serde(default)]
    pub copy: Vec<String>,
    #[serde(default)]
    pub link: Vec<String>,
    #[serde(default)]
    pub editors: BTreeMap<String, EditorConfig>,
}

impl GlobalConfig {
    pub fn path() -> Result<PathBuf, GlobalConfigError> {
        let config_dir = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .ok_or(GlobalConfigError::NoConfigDir)?;

        Ok(config_dir.join("grove").join("config.toml"))
    }

    pub fn load() -> Result<Self, GlobalConfigError> {
        let path = Self::path()?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path).map_err(|e| GlobalConfigError::Read {
            path: path.clone(),
            source: e,
        })?;

        toml::from_str(&contents).map_err(|e| GlobalConfigError::Parse { path, source: e })
    }

    pub fn save(&self) -> Result<(), GlobalConfigError> {
        let path = Self::path()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| GlobalConfigError::Write {
                path: path.clone(),
                source: e,
            })?;
        }

        let contents = toml::to_string_pretty(self)?;
        fs::write(&path, contents).map_err(|e| GlobalConfigError::Write { path, source: e })?;

        Ok(())
    }

    pub fn add_editor(
        &mut self,
        id: String,
        name: String,
        command: String,
        copy: Vec<String>,
        link: Vec<String>,
    ) -> Result<(), GlobalConfigError> {
        if self.editors.contains_key(&id) {
            return Err(GlobalConfigError::EditorAlreadyExists(id));
        }

        self.editors.insert(
            id,
            EditorConfig {
                name,
                command,
                copy,
                link,
            },
        );
        Ok(())
    }

    pub fn remove_editor(&mut self, id: &str) -> Result<(), GlobalConfigError> {
        if self.editors.remove(id).is_none() {
            return Err(GlobalConfigError::EditorNotFound(id.to_string()));
        }
        Ok(())
    }

    pub fn get_editor(&self, id: &str) -> Result<&EditorConfig, GlobalConfigError> {
        self.editors
            .get(id)
            .ok_or_else(|| GlobalConfigError::EditorNotFound(id.to_string()))
    }

    /// Merge global-level `copy` with editor-specific `copy`, deduplicating.
    pub fn resolved_copy(&self, editor_id: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for path in &self.copy {
            if seen.insert(path.clone()) {
                result.push(path.clone());
            }
        }

        if let Some(editor) = self.editors.get(editor_id) {
            for path in &editor.copy {
                if seen.insert(path.clone()) {
                    result.push(path.clone());
                }
            }
        }

        result
    }

    /// Merge global-level `link` with editor-specific `link`, deduplicating.
    pub fn resolved_link(&self, editor_id: &str) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();

        for path in &self.link {
            if seen.insert(path.clone()) {
                result.push(path.clone());
            }
        }

        if let Some(editor) = self.editors.get(editor_id) {
            for path in &editor.link {
                if seen.insert(path.clone()) {
                    result.push(path.clone());
                }
            }
        }

        result
    }
}
