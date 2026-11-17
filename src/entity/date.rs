use async_graphql::NewType;
use jiff::{civil, civil::Date as CivilDate};
use serde::Deserialize;

/// ISO 8601 calendar date without timezone.
/// Format: %Y-%m-%d
///
/// # Examples
///
/// * `1994-11-13`
/// * `2000-02-24`
#[derive(Debug, Clone, Copy, Deserialize, NewType)]
#[serde(try_from = "u16")]
#[graphql(name = "NaiveDate")]
pub(super) struct Date(CivilDate);

// ClickHouse Date stores an unsigned day count since 1970-01-01.
impl TryFrom<u16> for Date {
    type Error = jiff::Error;

    fn try_from(days: u16) -> Result<Self, Self::Error> {
        civil::date(1970, 1, 1)
            .checked_add(jiff::Span::new().days(i64::from(days)))
            .map(Self)
    }
}

#[cfg(test)]
mod tests {
    use async_graphql::{ScalarType, Value};

    use super::*;

    #[test]
    fn clickhouse_dates_preserve_day_counts_and_nulls() {
        for (days, expected) in [
            (0, "1970-01-01"),
            (11016, "2000-02-29"),
            (65535, "2149-06-06"),
        ] {
            let date: Option<Date> = serde_json::from_value(serde_json::json!(days)).unwrap();
            assert_eq!(date.unwrap().to_value(), Value::String(expected.into()));
        }
        assert!(
            serde_json::from_str::<Option<Date>>("null")
                .unwrap()
                .is_none()
        );
        for invalid in ["-1", "65536", "1.5", "\"2000-02-29\""] {
            assert!(serde_json::from_str::<Option<Date>>(invalid).is_err());
        }
    }

    #[test]
    fn graphql_dates_preserve_format_and_validation() {
        for date in ["1970-01-01", "2000-02-29", "2149-06-06"] {
            let value = Value::String(date.into());
            assert_eq!(Date::parse(value.clone()).unwrap().to_value(), value);
        }
        for invalid in ["2001-02-29", "2026-13-01", "2026-11-17T00:00:00Z"] {
            assert!(Date::parse(Value::String(invalid.into())).is_err());
        }
        assert!(Date::parse(Value::Null).is_err());
        assert!(Date::parse(Value::Number(0.into())).is_err());
    }
}
