use clickhouse::query::Query;

use crate::query::sort::SortDirection;

pub trait Column: Copy + Send + Sync + 'static {
    fn as_sql(&self) -> &'static str;
    fn kind(&self) -> ValueKind; // what type of value this column accepts
}

pub trait Table: Send + Sync + 'static {
    type Col: Column;
    /// Table to read, optionally with an alias for [`Self::JOINS`] (`"my_table AS t"`).
    const NAME: &'static str;
    /// Unique column used as the final ORDER BY key so OFFSET paging is stable.
    const TIE_BREAKER: &'static str;
    /// Columns to select. Filters, sorts and the row struct use these output names,
    /// and the row struct must match their order. Defaults to every column.
    const SELECT: &'static str = "*";
    /// `JOIN` clauses placed after `FROM`. Defaults to none.
    const JOINS: &'static str = "";
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    Str,
    U64,
    I64,
    F64,
    StrArray,
    StrArrayArray,
    U64Array,
}

/// Separator placed between fields when building the search text, so a match
/// can't span two fields. It's the ASCII "unit separator", which won't appear in data.
const SEARCH_SEP: &str = r"'\x1F'";

/// SQL expression turning a text column into a single string, or `None` if
/// the column isn't text.
fn as_text_sql<C: Column>(col: C) -> Option<String> {
    let name = col.as_sql();
    match col.kind() {
        // `ifNull` because one NULL makes the whole concatenation NULL, which would
        // drop the row from every search. It's a no-op on non-Nullable columns.
        ValueKind::Str => Some(format!("ifNull({name}, '')")),
        ValueKind::StrArray => Some(format!("arrayStringConcat({name}, {SEARCH_SEP})")),
        ValueKind::StrArrayArray => Some(format!(
            "arrayStringConcat(arrayFlatten({name}), {SEARCH_SEP})"
        )),
        ValueKind::U64 | ValueKind::I64 | ValueKind::F64 | ValueKind::U64Array => None,
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Str(String),
    U64(u64),
    I64(i64),
    F64(f64),
    U64Array(Vec<u64>),
}
impl Value {
    fn kind(&self) -> ValueKind {
        match self {
            Value::Str(_) => ValueKind::Str,
            Value::U64(_) => ValueKind::U64,
            Value::I64(_) => ValueKind::I64,
            Value::F64(_) => ValueKind::F64,
            Value::U64Array(_) => ValueKind::U64Array,
        }
    }
}

#[derive(Clone, Copy)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Op {
    fn as_sql(self) -> &'static str {
        match self {
            Op::Eq => "=",
            Op::Ne => "!=",
            Op::Lt => "<",
            Op::Le => "<=",
            Op::Gt => ">",
            Op::Ge => ">=",
        }
    }
}

pub enum Filter<C: Column> {
    /// `col op ?`
    Compare { col: C, op: Op, value: Value },
    /// Case-insensitive substring match of `term` against any of `cols`.
    Search { cols: Vec<C>, term: Value },
    /// `col IN ?`: the column equals one of `values`.
    In { col: C, values: Value },
}

impl<C: Column> Filter<C> {
    /// A comparison filter. Errors if the value's type doesn't match the column.
    #[allow(clippy::missing_errors_doc)]
    pub fn new(col: C, op: Op, value: Value) -> Result<Self, String> {
        if col.kind() != value.kind() {
            return Err(format!(
                "column {} expects {:?}, got {:?}",
                col.as_sql(),
                col.kind(),
                value.kind()
            ));
        }
        Ok(Self::Compare { col, op, value })
    }

    /// A text search filter. Errors if `cols` is empty or any column isn't text.
    #[allow(clippy::missing_errors_doc)]
    pub fn search(cols: Vec<C>, term: String) -> Result<Self, String> {
        if cols.is_empty() {
            return Err("search needs at least one column".to_string());
        }
        if let Some(col) = cols.iter().find(|c| as_text_sql(**c).is_none()) {
            return Err(format!("column {} is not searchable", col.as_sql()));
        }
        Ok(Self::Search {
            cols,
            term: Value::Str(term),
        })
    }

    /// An `IN` filter over a `U64` column. Errors if `col` isn't `U64`.
    #[allow(clippy::missing_errors_doc)]
    pub fn in_u64(col: C, values: Vec<u64>) -> Result<Self, String> {
        if col.kind() != ValueKind::U64 {
            return Err(format!(
                "column {} expects U64, got {:?}",
                col.as_sql(),
                col.kind()
            ));
        }
        Ok(Self::In {
            col,
            values: Value::U64Array(values),
        })
    }

    /// The SQL condition, with one `?` placeholder for [`Self::value`].
    fn to_sql(&self) -> String {
        match self {
            Self::Compare { col, op, .. } => format!("{} {} ?", col.as_sql(), op.as_sql()),
            Self::In { col, .. } => format!("{} IN ?", col.as_sql()),
            Self::Search { cols, .. } => {
                let fields = cols
                    .iter()
                    .filter_map(|c| as_text_sql(*c))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "positionCaseInsensitiveUTF8(concatWithSeparator({SEARCH_SEP}, {fields}), ?) > 0"
                )
            }
        }
    }

    /// The value bound to this filter's `?`.
    fn value(&self) -> &Value {
        match self {
            Self::Compare { value, .. }
            | Self::Search { term: value, .. }
            | Self::In { values: value, .. } => value,
        }
    }
}

pub struct OrderBy<C: Column> {
    pub col: C,
    pub dir: SortDirection,
}

impl<C: Column> OrderBy<C> {
    /// One `ORDER BY` term per column, all in the same direction, in order.
    #[must_use]
    pub fn each(cols: &[C], dir: SortDirection) -> Vec<Self> {
        cols.iter().map(|&col| Self { col, dir }).collect()
    }
}

pub struct PageSpec<T: Table> {
    pub filters: Vec<Filter<T::Col>>,
    pub order_by: Vec<OrderBy<T::Col>>,
    pub offset: u64,
    pub limit: u64,
}

macro_rules! columns {
    ($name:ident { $($variant:ident => $sql:literal : $kind:ident),* $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name { $($variant),* }
        impl Column for $name {
            fn as_sql(&self) -> &'static str {
                match self { $(Self::$variant => $sql),* }
            }
            fn kind(&self) -> ValueKind {
                match self { $(Self::$variant => ValueKind::$kind),* }
            }
        }
    };
}
pub(crate) use columns;

/// Declares an enum whose variants are a subset of a [`columns!`] enum, with the
/// same names, and generates `as_col()` mapping each variant to its column.
///
/// Attributes (docs, derives, `#[graphql(...)]`) are passed through, so this works
/// for GraphQL enums that expose a choice of columns to clients.
///
/// ```ignore
/// column_subset! {
///     #[derive(Clone, Copy, Enum)]
///     pub enum SearchField => CredibleSetCol { StudyId, DiseaseNames }
/// }
/// ```
macro_rules! column_subset {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident => $col:ident { $($variant:ident),* $(,)? }
    ) => {
        $(#[$meta])*
        $vis enum $name { $($variant),* }
        impl $name {
            /// The column this variant refers to.
            const fn as_col(self) -> $col {
                match self { $(Self::$variant => $col::$variant),* }
            }
        }
    };
}
pub(crate) use column_subset;

const TEMPLATE: &str = include_str!("query.sql");

/// Table-independent, ready for batching.
pub struct Rendered {
    sql: String,
    offset: u64,
    limit: u64,
    where_binds: Vec<Value>,
}

impl<T: Table> PageSpec<T> {
    #[must_use]
    pub fn render(&self) -> Rendered {
        let where_sql = if self.filters.is_empty() {
            "1".to_string()
        } else {
            self.filters
                .iter()
                .map(Filter::to_sql)
                .collect::<Vec<_>>()
                .join(" AND ")
        };

        let mut order: Vec<String> = self
            .order_by
            .iter()
            .map(|o| format!("{} {}", o.col.as_sql(), o.dir.as_sql()))
            .collect();
        order.push(format!("{} ASC", T::TIE_BREAKER));

        let sql = TEMPLATE
            .replace("{{select}}", T::SELECT)
            .replace("{{table}}", T::NAME)
            .replace("{{joins}}", T::JOINS)
            .replace("{{where}}", &where_sql)
            .replace("{{order_by}}", &order.join(", "));
        debug_assert!(!sql.contains("{{"), "unfilled template slot");

        Rendered {
            sql,
            offset: self.offset,
            limit: self.limit,
            where_binds: self.filters.iter().map(|f| f.value().clone()).collect(),
        }
    }
}

fn bind_value(q: Query, v: &Value) -> Query {
    match v {
        Value::Str(s) => q.bind(s.as_str()),
        Value::U64(n) => q.bind(*n),
        Value::I64(n) => q.bind(*n),
        Value::F64(n) => q.bind(*n),
        Value::U64Array(ns) => q.bind(ns.as_slice()),
    }
}

pub fn build(ch: &clickhouse::Client, items: &[Rendered]) -> Query {
    let sql = items
        .iter()
        .map(|r| r.sql.as_str())
        .collect::<Vec<_>>()
        .join(" UNION ALL ");

    items.iter().enumerate().fold(ch.query(&sql), |q, (i, r)| {
        let q = q.bind(r.offset).bind(r.limit).bind(i as u64);
        r.where_binds.iter().fold(q, bind_value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fake table used only by the tests.
    columns!(TestCol {
        Id    => "id"    : Str,
        Score => "score" : F64,
        Count => "count" : U64,
        Tags  => "tags"  : StrArray,
    });

    struct TestTable;
    impl Table for TestTable {
        type Col = TestCol;
        const NAME: &'static str = "test_table";
        const TIE_BREAKER: &'static str = "id";
    }

    fn spec(filters: Vec<Filter<TestCol>>, order_by: Vec<OrderBy<TestCol>>) -> PageSpec<TestTable> {
        PageSpec {
            filters,
            order_by,
            offset: 0,
            limit: 10,
        }
    }

    #[test]
    fn no_filters_renders_where_1() {
        let r = spec(vec![], vec![]).render();
        assert!(r.sql.contains("WHERE 1"), "sql was:\n{}", r.sql);
        assert!(r.where_binds.is_empty());
    }

    #[test]
    fn defaults_select_everything_without_joins() {
        let r = spec(vec![], vec![]).render();
        assert!(
            r.sql.contains("SELECT *\n        FROM test_table\n"),
            "sql was:\n{}",
            r.sql
        );
    }

    #[test]
    fn select_and_joins_fill_their_slots() {
        struct Joined;
        impl Table for Joined {
            type Col = TestCol;
            const NAME: &'static str = "test_table AS t";
            const TIE_BREAKER: &'static str = "id";
            const SELECT: &'static str = "t.id AS id, o.name AS name";
            const JOINS: &'static str = "LEFT JOIN other AS o ON t.id = o.id";
        }
        let r = PageSpec::<Joined> {
            filters: vec![],
            order_by: vec![],
            offset: 0,
            limit: 10,
        }
        .render();
        assert!(
            r.sql.contains("SELECT t.id AS id, o.name AS name"),
            "sql was:\n{}",
            r.sql
        );
        assert!(
            r.sql
                .contains("FROM test_table AS t\n        LEFT JOIN other AS o ON t.id = o.id"),
            "sql was:\n{}",
            r.sql
        );
    }

    #[test]
    fn compare_rejects_value_of_wrong_type() {
        let result = Filter::new(TestCol::Score, Op::Eq, Value::Str("high".to_string()));

        let Err(msg) = result else {
            panic!("expected an error for a Str value on an F64 column");
        };
        assert_eq!(msg, "column score expects F64, got Str");
    }

    #[test]
    fn compare_accepts_value_of_right_type() {
        let result = Filter::new(TestCol::Score, Op::Gt, Value::F64(1.0));
        assert!(result.is_ok());
    }

    #[test]
    fn search_wraps_text_columns_in_if_null() {
        let f = Filter::search(vec![TestCol::Id, TestCol::Tags], "x".to_string()).unwrap();
        let sql = f.to_sql();
        assert!(sql.contains("ifNull(id, '')"), "sql was:\n{sql}");
        assert!(sql.contains("arrayStringConcat(tags,"), "sql was:\n{sql}");
    }

    #[test]
    fn in_u64_renders_in() {
        let f = Filter::in_u64(TestCol::Count, vec![1, 2]).unwrap();
        assert_eq!(f.to_sql(), "count IN ?");
    }

    #[test]
    fn in_u64_rejects_non_u64_column() {
        let Err(msg) = Filter::in_u64(TestCol::Id, vec![1]) else {
            panic!("expected an error for a Str column");
        };
        assert_eq!(msg, "column id expects U64, got Str");
    }
}
