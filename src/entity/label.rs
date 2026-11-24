//! A vocabulary value decoded from a ClickHouse `Enum8` and served as a GraphQL `String`.

use std::borrow::Cow;

use async_graphql::{
    ContextSelectionSet, OutputType, Positioned, ServerResult, Value, parser::types::Field,
    registry::Registry,
};
use serde::Deserialize;

/// A `repr(i8)` vocabulary enum, served as its data spelling (`IntoStaticStr`), a GraphQL `String`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Label<T>(pub T);

impl<T> OutputType for Label<T>
where
    T: Copy + Send + Sync,
    &'static str: From<T>,
{
    fn type_name() -> Cow<'static, str> { <str as OutputType>::type_name() }

    fn create_type_info(registry: &mut Registry) -> String {
        <str as OutputType>::create_type_info(registry)
    }

    async fn resolve(
        &self,
        _: &ContextSelectionSet<'_>,
        _field: &Positioned<Field>,
    ) -> ServerResult<Value> {
        Ok(Value::String(<&'static str>::from(self.0).to_owned()))
    }
}
