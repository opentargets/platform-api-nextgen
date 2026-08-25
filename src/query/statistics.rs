pub trait Statistics: Sized {
    type Stats: async_graphql::OutputType;
    fn compute(items: &[Self]) -> Self::Stats;
}
