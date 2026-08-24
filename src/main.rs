use clap::Parser;
use platform_api::{
    AppState,
    config::{Args, Config},
    logging,
    server::{self, graphql::build_schema},
};

#[tokio::main]
async fn main() {
    let config = Config::load(Args::parse().config.as_deref());
    logging::init(&config);

    let app_state = AppState::new(config);
    let schema = build_schema(&app_state);

    server::http::serve(app_state, schema).await;
}
