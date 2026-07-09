pub const MAX_PAGE_SIZE: usize = 1000;

#[derive(Debug, Clone, Copy, async_graphql::InputObject)]
pub struct Page {
    #[graphql(default = 0)]
    pub index: usize,
    #[graphql(default = 10, validator(minimum = 1, maximum = 1000))]
    pub size: usize,
}

impl Default for Page {
    fn default() -> Self { Self { index: 0, size: 10 } }
}
