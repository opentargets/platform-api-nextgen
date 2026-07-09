//! The plugin registry, which holds all the plugins registered in the API.

use std::collections::HashMap;

use crate::{config::Config, plugin::plugin::Plugin};

/// The plugins available to the API.
///
/// Built once at startup from configuration; membership is fixed for the
/// lifetime of the process.
#[derive(Clone, Debug, Default)]
pub struct PluginRegistry(HashMap<String, Plugin>);

impl PluginRegistry {
    /// Create a new plugin registry from the configuration.
    #[must_use]
    pub fn new(config: &Config) -> Self { config.plugins.iter().cloned().collect() }

    /// Get a plugin by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Plugin> { self.0.get(name) }

    /// Iterate over all registered plugins.
    pub fn iter(&self) -> impl Iterator<Item = &Plugin> { self.0.values() }

    /// The number of registered plugins.
    #[must_use]
    pub fn len(&self) -> usize { self.0.len() }

    /// Check if the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool { self.0.is_empty() }
}

impl FromIterator<Plugin> for PluginRegistry {
    fn from_iter<I: IntoIterator<Item = Plugin>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|plugin| (plugin.name.clone(), plugin))
                .collect(),
        )
    }
}
