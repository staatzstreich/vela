use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("Keyring error: {0}")]
    Keyring(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMethod {
    Key,
    Password,
}

impl AuthMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Password => "password",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
    pub key_path: Option<String>,
    /// Remote directory to switch into right after connecting.
    /// Empty / absent means the server's login default is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_path: Option<String>,
    /// Local directory to navigate to in the left panel right after connecting.
    /// Empty / absent means the current local directory is kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_start_path: Option<String>,
    /// Whether a password is stored in the OS keychain for this profile.
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_saved_password: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProfileStore {
    #[serde(rename = "profile", default)]
    pub profiles: Vec<Profile>,
}

impl ProfileStore {
    pub fn load() -> Result<Self, ConfigError> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)?;
        let store = toml::from_str(&content)?;
        Ok(store)
    }

    pub fn save(&self) -> Result<(), ConfigError> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn add(&mut self, profile: Profile) {
        self.profiles.push(profile);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.profiles.len() {
            self.profiles.remove(index);
        }
    }

    pub fn update(&mut self, index: usize, profile: Profile) {
        if index < self.profiles.len() {
            self.profiles[index] = profile;
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".config").join("vela").join("profiles.toml")
}

// ---------------------------------------------------------------------------
// Keyring helpers — store/load/delete passwords via OS keychain
// ---------------------------------------------------------------------------

const KEYRING_SERVICE: &str = "vela";

/// Build the keyring entry for a profile: service="vela", user=profile_name.
fn keyring_entry(profile_name: &str) -> Result<keyring::Entry, ConfigError> {
    keyring::Entry::new(KEYRING_SERVICE, profile_name)
        .map_err(|e| ConfigError::Keyring(e.to_string()))
}

/// Store a password in the OS keychain for the given profile name.
pub fn save_password(profile_name: &str, password: &str) -> Result<(), ConfigError> {
    let entry = keyring_entry(profile_name)?;
    entry
        .set_password(password)
        .map_err(|e| ConfigError::Keyring(e.to_string()))
}

/// Load a password from the OS keychain. Returns `None` if not found.
pub fn load_password(profile_name: &str) -> Result<Option<String>, ConfigError> {
    let entry = keyring_entry(profile_name)?;
    match entry.get_password() {
        Ok(pw) => Ok(Some(pw)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(ConfigError::Keyring(e.to_string())),
    }
}

/// Delete a password from the OS keychain. Ignores "not found" errors.
pub fn delete_password(profile_name: &str) -> Result<(), ConfigError> {
    let entry = keyring_entry(profile_name)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(ConfigError::Keyring(e.to_string())),
    }
}
