//! Plugin type definition.

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub base_url: String,
}

impl Plugin {
    fn is_valid_name(name: &str) -> bool {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > 63 {
            return false;
        }
        let is_alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        if !is_alnum(bytes[0]) || !is_alnum(bytes[bytes.len() - 1]) {
            return false;
        }
        bytes.iter().all(|&b| is_alnum(b) || b == b'-')
    }

    fn is_valid_base_url(base_url: &str) -> bool {
        reqwest::Url::parse(base_url).is_ok()
            && (base_url.starts_with("http://") || base_url.starts_with("https://"))
    }

    /// Creates a new plugin instance with the given name and base URL.
    ///
    /// # Errors
    /// Returns a `PluginError` if the name or base URL is invalid.
    pub fn new(name: &str, base_url: &str) -> Result<Self, PluginError> {
        if !Self::is_valid_name(name) {
            return Err(PluginError::InvalidName(name.to_string()));
        }

        let trimmed_base_url = base_url.trim_end_matches('/');
        if !Self::is_valid_base_url(trimmed_base_url) {
            return Err(PluginError::InvalidBaseUrl);
        }
        Ok(Self {
            name: name.to_string(),
            base_url: trimmed_base_url.to_string(),
        })
    }
}
