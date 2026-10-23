use clickhouse::query::Query;

use crate::query::sort::SortDirection;

pub trait Column: Copy + Send + Sync + 'static {
    fn as_sql(&self) -> &'static str;
    fn kind(&self) -> ValueKind; // what type of value this column accepts
}

pub trait Table: Send + Sync + 'static {
    type Col: Column;
    const NAME: &'static str;
    /// Unique column used as the final ORDER BY key so OFFSET paging is stable.
    const TIE_BREAKER: &'static str;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    Str,
    U64,
    I64,
    F64,
}

#[derive(Clone, Debug)]
pub enum Value {
    Str(String),
    U64(u64),
    I64(i64),
    F64(f64),
}
impl Value {
    fn kind(&self) -> ValueKind {
        match self {
            Value::Str(_) => ValueKind::Str,
            Value::U64(_) => ValueKind::U64,
            Value::I64(_) => ValueKind::I64,
            Value::F64(_) => ValueKind::F64,
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

pub struct Filter<C: Column> {
    col: C,
    op: Op,
    value: Value,
}

impl<C: Column> Filter<C> {
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
        Ok(Self { col, op, value })
    }
}

pub struct OrderBy<C: Column> {
    pub col: C,
    pub dir: SortDirection,
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
                .map(|f| format!("{} {} ?", f.col.as_sql(), f.op.as_sql()))
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
            .replace("{{table}}", T::NAME)
            .replace("{{where}}", &where_sql)
            .replace("{{order_by}}", &order.join(", "));
        debug_assert!(!sql.contains("{{"), "unfilled template slot");

        Rendered {
            sql,
            offset: self.offset,
            limit: self.limit,
            where_binds: self.filters.iter().map(|f| f.value.clone()).collect(),
        }
    }
}

fn bind_value(q: Query, v: &Value) -> Query {
    match v {
        Value::Str(s) => q.bind(s.as_str()),
        Value::U64(n) => q.bind(*n),
        Value::I64(n) => q.bind(*n),
        Value::F64(n) => q.bind(*n),
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
