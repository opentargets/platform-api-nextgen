pub trait Searchable {
    fn matches_search(&self, needle: &str) -> bool;
}
