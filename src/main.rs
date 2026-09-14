use clap::Parser;
use platform_api::{
    AppState,
    config::{Args, Config},
    logging, server,
};

#[tokio::main]
async fn main() {
    let config = Config::load(Args::parse().config.as_deref());
    logging::init(&config);

    let app_state = AppState::new(config);

    server::http::serve(app_state).await;
}
