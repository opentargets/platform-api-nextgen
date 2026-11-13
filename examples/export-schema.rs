use async_graphql::{EmptyMutation, EmptySubscription};
use platform_api::product::{ApiSchema, CurrentProduct, Product};

fn main() {
    let schema = ApiSchema::build(
        <CurrentProduct as Product>::Query::default(),
        EmptyMutation,
        EmptySubscription,
    )
    .finish();

    print!("{}", schema.sdl());
}
