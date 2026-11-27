//! Build version of the API.

pub const API_VERSION: &str = match option_env!("APP_VERSION") {
    Some(v) if !v.is_empty() => v,
    _ => env!("CARGO_PKG_VERSION"),
};

#[cfg(test)]
mod tests {
    use super::API_VERSION;

    #[test]
    fn api_version_is_never_empty() {
        assert!(!API_VERSION.is_empty());
    }
}
