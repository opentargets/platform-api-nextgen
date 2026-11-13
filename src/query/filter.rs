//! Filters for GraphQL queries.

use async_graphql::InputObject;

use crate::query::sql::{self, Column, Op, Value};

/// Per-entity filter: do we keep this row?
pub trait Filter<T> {
    fn matches(&self, item: &T) -> bool;
}

/// In `ClickHouse`: the filter as SQL conditions on columns `C`, combined with `AND`.
pub trait SqlFilter<C: Column> {
    /// # Errors
    /// If a condition doesn't fit its column's type.
    fn to_sql(&self) -> Result<Vec<sql::Filter<C>>, String>;
}

/// Null-object filter.
///
/// Used when we don't want to define filters.
pub struct NoFilter;
impl<T> Filter<T> for NoFilter {
    fn matches(&self, _: &T) -> bool { true }
}

/// Filter for a string field.
#[derive(Debug, Clone, Default, Eq, PartialEq, Hash, InputObject)]
pub struct StringFilter {
    pub eq: Option<String>,
    pub ne: Option<String>,
    pub contains: Option<String>,
    pub not_contains: Option<String>,
    #[graphql(name = "in")]
    pub in_list: Option<Vec<String>>,
    pub is_null: Option<bool>,
}

impl StringFilter {
    #[must_use]
    pub fn matches(&self, value: Option<&str>) -> bool {
        if let Some(want_null) = self.is_null
            && value.is_none() != want_null
        {
            return false;
        }
        let Some(v) = value else {
            return !(self.eq.is_some()
                || self.ne.is_some()
                || self.contains.is_some()
                || self.not_contains.is_some()
                || self.in_list.is_some());
        };
        self.eq.as_deref().is_none_or(|x| v == x)
            && self.ne.as_deref().is_none_or(|x| v != x)
            && self
                .not_contains
                .as_deref()
                .is_none_or(|x| !v.to_lowercase().contains(&x.to_lowercase()))
            && self
                .contains
                .as_deref()
                .is_none_or(|x| v.to_lowercase().contains(&x.to_lowercase()))
            && self
                .in_list
                .as_ref()
                .is_none_or(|xs| xs.iter().any(|x| x == v))
    }

    /// The same filter as SQL conditions on `col`, one per operation that is set.
    ///
    /// NULLs behave as in [`Self::matches`]: in SQL any comparison with NULL drops the row,
    /// so only `isNull: true` keeps a NULL value.
    ///
    /// # Errors
    /// If `col` isn't a `Str` column.
    pub fn to_sql<C: Column>(&self, col: C) -> Result<Vec<sql::Filter<C>>, String> {
        let mut out = Vec::new();
        if let Some(null) = self.is_null {
            out.push(sql::Filter::is_null(col, null));
        }
        if let Some(v) = &self.eq {
            out.push(sql::Filter::new(col, Op::Eq, Value::Str(v.clone()))?);
        }
        if let Some(v) = &self.ne {
            out.push(sql::Filter::new(col, Op::Ne, Value::Str(v.clone()))?);
        }
        if let Some(v) = &self.contains {
            out.push(sql::Filter::contains(col, v.clone(), false)?);
        }
        if let Some(v) = &self.not_contains {
            out.push(sql::Filter::contains(col, v.clone(), true)?);
        }
        if let Some(vs) = &self.in_list {
            out.push(sql::Filter::in_str(col, vs.clone())?);
        }
        Ok(out)
    }
}

/// Filter for an integer field.
#[derive(Debug, InputObject)]
pub struct IntFilter {
    pub eq: Option<i64>,
    pub ne: Option<i64>,
    pub gt: Option<i64>,
    pub lt: Option<i64>,
    pub gte: Option<i64>,
    pub lte: Option<i64>,
    #[graphql(name = "in")]
    pub in_list: Option<Vec<i64>>,
    pub is_null: Option<bool>,
}

impl IntFilter {
    #[must_use]
    pub fn matches(&self, value: Option<i64>) -> bool {
        if let Some(want_null) = self.is_null
            && value.is_none() != want_null
        {
            return false;
        }
        let Some(v) = value else {
            return !(self.eq.is_some()
                || self.ne.is_some()
                || self.gt.is_some()
                || self.lt.is_some()
                || self.gte.is_some()
                || self.lte.is_some()
                || self.in_list.is_some());
        };
        self.eq.is_none_or(|x| v == x)
            && self.ne.is_none_or(|x| v != x)
            && self.gt.is_none_or(|x| v > x)
            && self.lt.is_none_or(|x| v < x)
            && self.gte.is_none_or(|x| v >= x)
            && self.lte.is_none_or(|x| v <= x)
            && self.in_list.as_ref().is_none_or(|xs| xs.contains(&v))
    }
}

/// Filter for a float field.
#[derive(Debug, InputObject)]
pub struct FloatFilter {
    pub eq: Option<f64>,
    pub ne: Option<f64>,
    pub gt: Option<f64>,
    pub lt: Option<f64>,
    pub gte: Option<f64>,
    pub lte: Option<f64>,
    #[graphql(name = "in")]
    pub in_list: Option<Vec<f64>>,
    pub is_null: Option<bool>,
}

/// Compare two floats for equality, allowing for some small error.
fn nearly_equal(a: f64, b: f64) -> bool {
    const REL: f64 = 1e-9;
    const ABS: f64 = 1e-12;
    #[allow(clippy::float_cmp)]
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    diff <= ABS || diff <= REL * a.abs().max(b.abs())
}

impl FloatFilter {
    #[must_use]
    pub fn matches(&self, value: Option<f64>) -> bool {
        if let Some(want_null) = self.is_null
            && value.is_none() != want_null
        {
            return false;
        }
        let Some(v) = value else {
            return !(self.eq.is_some()
                || self.ne.is_some()
                || self.gt.is_some()
                || self.lt.is_some()
                || self.gte.is_some()
                || self.lte.is_some()
                || self.in_list.is_some());
        };
        self.eq.is_none_or(|x| nearly_equal(v, x))
            && self.ne.is_none_or(|x| !nearly_equal(v, x))
            && self.gt.is_none_or(|x| v > x)
            && self.lt.is_none_or(|x| v < x)
            && self.gte.is_none_or(|x| v >= x)
            && self.lte.is_none_or(|x| v <= x)
            && self.in_list.as_ref().is_none_or(|xs| xs.contains(&v))
    }
}

/// Filter for a boolean field.
#[derive(Debug, InputObject)]
pub struct BoolFilter {
    pub eq: Option<bool>,
    pub is_null: Option<bool>,
}

impl BoolFilter {
    #[must_use]
    pub fn matches(&self, value: Option<bool>) -> bool {
        if let Some(want_null) = self.is_null
            && value.is_none() != want_null
        {
            return false;
        }
        let Some(v) = value else {
            return self.eq.is_none();
        };
        self.eq.is_none_or(|x| v == x)
    }
}

/// Filter for an array field.
#[derive(Debug, InputObject)]
pub struct ArrayFilter {
    pub contains_any: Option<Vec<String>>,
    pub contains_all: Option<Vec<String>>,
    pub is_empty: Option<bool>,
}

impl ArrayFilter {
    #[must_use]
    pub fn matches(&self, values: &[String]) -> bool {
        self.is_empty.is_none_or(|want| values.is_empty() == want)
            && self
                .contains_any
                .as_ref()
                .is_none_or(|wanted| wanted.iter().any(|w| values.iter().any(|v| v == w)))
            && self
                .contains_all
                .as_ref()
                .is_none_or(|wanted| wanted.iter().all(|w| values.iter().any(|v| v == w)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::sql::{ValueKind, columns};

    columns!(TestCol {
        Id    => "id"    : Str,
        Count => "count" : U64,
    });

    #[test]
    fn string_filter_ne_gives_one_condition() {
        let f = StringFilter {
            ne: Some("x".to_string()),
            ..StringFilter::default()
        };
        let conditions = f.to_sql(TestCol::Id).unwrap();
        assert_eq!(conditions.len(), 1);
    }

    #[test]
    fn string_filter_gives_one_condition_per_operation() {
        let f = StringFilter {
            eq: Some("a".to_string()),
            ne: Some("b".to_string()),
            contains: Some("c".to_string()),
            not_contains: Some("d".to_string()),
            in_list: Some(vec!["e".to_string()]),
            is_null: Some(false),
        };
        assert_eq!(f.to_sql(TestCol::Id).unwrap().len(), 6);
    }

    #[test]
    fn string_filter_empty_gives_no_conditions() {
        assert!(
            StringFilter::default()
                .to_sql(TestCol::Id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn string_filter_rejects_non_str_column() {
        let f = StringFilter {
            eq: Some("1".to_string()),
            ..StringFilter::default()
        };
        assert!(f.to_sql(TestCol::Count).is_err());
    }
}
