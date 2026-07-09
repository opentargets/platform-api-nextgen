//! Tracing initialisation.

use std::io::{self, IsTerminal};

use tracing_subscriber::{
    EnvFilter,
    fmt::{self, format::FmtSpan},
    prelude::*,
};

use crate::config::Config;

const BANNER: &str = r"
    ▒▒   ▄███▄ ▄▄▄▄  ▄▄▄▄▄ ▄▄  ▄▄  ██████ ▄▄▄  ▄▄▄▄   ▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄▄▄ ▄▄▄▄
  ██████ ██ ██ ██▄█▀ ██▄▄  ███▄██    ██  ██▀██ ██▄█▄ ██ ▄▄ ██▄▄    ██  ███▄▄
▒▒▒▒▒▒   ▀███▀ ██    ██▄▄▄ ██ ▀██    ██  ██▀██ ██ ██ ▀███▀ ██▄▄▄   ██  ▄▄██▀
  ██";

pub fn init(config: &Config) {
    let filter = EnvFilter::new(format!(
        "platform_api={},clickhouse=warn,hyper_util=warn",
        config.log_level
    ));
    let force_pretty = std::env::var_os("LOG_PRETTY").is_some();

    if force_pretty || io::stdout().is_terminal() {
        println!(
            "{BANNER}{:>72}\n",
            concat!("API Version ", env!("CARGO_PKG_VERSION"))
        );
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().pretty().with_span_events(FmtSpan::CLOSE))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_span_events(FmtSpan::CLOSE),
            )
            .init();
    }
}
