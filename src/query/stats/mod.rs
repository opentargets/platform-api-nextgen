pub mod distribution;
pub mod sumstats;

pub trait HasStats: Sized {
    type Stats: async_graphql::OutputType + Clone;
}

pub trait ComputeStats: HasStats {
    fn compute(items: &[Self]) -> Self::Stats;
}
