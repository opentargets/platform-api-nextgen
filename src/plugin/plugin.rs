//! Plugin type definition.

use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::error::PluginError;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Plugin {
    pub name: String,
    pub base_url: Url,
}

impl Plugin {
    fn validate_name(name: &str) -> Result<(), PluginError> {
        let bytes = name.as_bytes();
        let is_alnum = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
        let ok = (1..=63).contains(&bytes.len())
            && is_alnum(bytes[0])
            && is_alnum(bytes[bytes.len() - 1])
            && bytes.iter().all(|&b| is_alnum(b) || b == b'-');
        if !ok {
            return Err(PluginError::InvalidName(name.to_string()));
        }
        Ok(())
    }

    /// # Errors
    /// Returns `PluginError` if the name, base URL, or endpoint is invalid.
    pub fn new(name: &str, base_url: &str) -> Result<Self, PluginError> {
        Self::validate_name(name)?;
        let base_url = Url::parse(base_url)?;
        Ok(Self {
            name: name.to_string(),
            base_url,
        })
    }

    #[must_use]
    pub fn url(&self) -> Url { self.base_url.clone() }
}
