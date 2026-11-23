//! Checks the Rust enums decoded from ClickHouse `Enum8`/`Enum16` columns against a database.
//!
//! `examples/check-clickhouse-enums.rs` reads each binding's type with
//! `DESCRIBE (SELECT <expr> FROM <table>)` and compares the value lists.

use strum::IntoEnumIterator;

use crate::entity::{
    credible_set::FineMappingMethod, interaction::InteractionSourceDatabase, study::StudyType,
    variant::Chromosome,
};

/// A Rust enum and a ClickHouse expression it decodes.
pub struct EnumBinding {
    /// The Rust enum's name.
    pub rust: &'static str,
    /// Each variant's ClickHouse name and number.
    pub variants: fn() -> Vec<(&'static str, i16)>,
    /// The table the expression reads.
    pub table: &'static str,
    /// A SQL expression over `table` whose type is the enum.
    pub expr: &'static str,
}

macro_rules! binding {
    ($ty:ident, $table:literal, $expr:literal) => {
        EnumBinding {
            rust: stringify!($ty),
            variants: || {
                $ty::iter()
                    .map(|v| (<&'static str>::from(v), v as i16))
                    .collect()
            },
            table: $table,
            expr: $expr,
        }
    };
}

/// Every (Rust enum, ClickHouse column) pair the API decodes.
#[must_use]
pub fn bindings() -> Vec<EnumBinding> {
    vec![
        binding!(Chromosome, "variants", "chromosome"),
        binding!(Chromosome, "credible_sets", "chromosome"),
        binding!(Chromosome, "enhancer_to_gene", "chromosome"),
        binding!(Chromosome, "targets_by_region", "chromosome"),
        binding!(Chromosome, "study_credible_sets", "leadChromosome"),
        binding!(Chromosome, "variant_credible_sets", "leadChromosome"),
        binding!(Chromosome, "target_credible_sets", "leadChromosome"),
        binding!(
            Chromosome,
            "colocalisation",
            "tupleElement(arrayJoin(colocalisation), 'chromosome')"
        ),
        binding!(StudyType, "studies", "studyType"),
        binding!(StudyType, "credible_sets", "studyType"),
        binding!(StudyType, "variant_credible_sets", "studyType"),
        binding!(StudyType, "target_credible_sets", "studyType"),
        binding!(
            StudyType,
            "colocalisation",
            "tupleElement(arrayJoin(colocalisation), 'rightStudyType')"
        ),
        binding!(FineMappingMethod, "credible_sets", "fineMappingMethod"),
        binding!(
            FineMappingMethod,
            "study_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            FineMappingMethod,
            "variant_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            FineMappingMethod,
            "target_credible_sets",
            "fineMappingMethod"
        ),
        binding!(
            InteractionSourceDatabase,
            "interaction",
            "tupleElement(arrayJoin(interactions), 'sourceDatabase')"
        ),
    ]
}

/// The `(name, number)` values of a ClickHouse `Enum8`/`Enum16` type, unwrapping `Nullable`,
/// `LowCardinality` and `Array`.
///
/// # Errors
/// Returns an error if the type is not an enum or is malformed.
pub fn parse_enum_type(type_name: &str) -> Result<Vec<(String, i16)>, String> {
    let mut s = type_name.trim();
    let mut wrappers = 0;
    while let Some(inner) = ["Nullable(", "LowCardinality(", "Array("]
        .iter()
        .find_map(|w| s.strip_prefix(w))
    {
        s = inner;
        wrappers += 1;
    }
    let body = s
        .strip_prefix("Enum8(")
        .or_else(|| s.strip_prefix("Enum16("))
        .ok_or_else(|| format!("not an enum: {type_name}"))?;
    let mut chars = body.chars().peekable();
    let mut values = Vec::new();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next() != Some('\'') {
            return Err(format!("expected a quoted name in {type_name}"));
        }
        let mut name = String::new();
        loop {
            match chars.next() {
                Some('\\') => match chars.next() {
                    Some('n') => name.push('\n'),
                    Some('t') => name.push('\t'),
                    Some('0') => name.push('\0'),
                    Some(c) => name.push(c),
                    None => return Err(format!("unterminated name in {type_name}")),
                },
                Some('\'') => break,
                Some(c) => name.push(c),
                None => return Err(format!("unterminated name in {type_name}")),
            }
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.next() != Some('=') {
            return Err(format!("expected '=' after '{name}' in {type_name}"));
        }
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let mut number = String::new();
        while let Some(c) = chars.next_if(|c| *c == '-' || c.is_ascii_digit()) {
            number.push(c);
        }
        let number = number
            .parse::<i16>()
            .map_err(|e| format!("bad number for '{name}' in {type_name}: {e}"))?;
        values.push((name, number));
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        match chars.next() {
            Some(',') => {}
            Some(')') => break,
            _ => return Err(format!("expected ',' or ')' in {type_name}")),
        }
    }
    let rest: String = chars.filter(|c| !c.is_whitespace()).collect();
    if rest != ")".repeat(wrappers) {
        return Err(format!("unexpected trailing '{rest}' in {type_name}"));
    }
    Ok(values)
}

/// Differences between a Rust enum's values and a ClickHouse enum's values.
#[must_use]
pub fn mismatches(rust: &[(&str, i16)], clickhouse: &[(String, i16)]) -> Vec<String> {
    let mut out = Vec::new();
    for (name, number) in clickhouse {
        match rust.iter().find(|(n, _)| n == name) {
            None => out.push(format!("missing value '{name}' = {number}")),
            Some((_, r)) if r != number => out.push(format!(
                "number differs for '{name}': rust {r}, clickhouse {number}"
            )),
            Some(_) => {}
        }
    }
    for (name, number) in rust {
        if !clickhouse.iter().any(|(n, _)| n == name) {
            out.push(format!("extra value '{name}' = {number}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(v: &[(&str, i16)]) -> Vec<(String, i16)> {
        v.iter().map(|(n, i)| ((*n).to_string(), *i)).collect()
    }

    #[test]
    fn parses_enum8() {
        assert_eq!(
            parse_enum_type("Enum8('a' = 1, 'b' = 2)"),
            Ok(owned(&[("a", 1), ("b", 2)]))
        );
    }

    #[test]
    fn unwraps_nullable_and_reads_negative_numbers() {
        assert_eq!(
            parse_enum_type("Nullable(Enum8('-1' = -1, '0' = 0, '1' = 1))"),
            Ok(owned(&[("-1", -1), ("0", 0), ("1", 1)]))
        );
    }

    #[test]
    fn unwraps_array_and_low_cardinality_and_unescapes_quotes() {
        assert_eq!(
            parse_enum_type("Array(LowCardinality(Enum8('it\\'s' = 1)))"),
            Ok(owned(&[("it's", 1)]))
        );
    }

    #[test]
    fn rejects_non_enum_types() {
        assert!(parse_enum_type("String").is_err());
    }

    #[test]
    fn reports_missing_extra_and_renumbered_values() {
        let found = mismatches(
            &[("a", 1), ("b", 2), ("x", 9)],
            &owned(&[("a", 1), ("b", 3), ("c", 4)]),
        );
        assert_eq!(
            found,
            [
                "number differs for 'b': rust 2, clickhouse 3",
                "missing value 'c' = 4",
                "extra value 'x' = 9",
            ]
        );
    }

    #[test]
    fn bindings_list_every_variant() {
        for b in bindings() {
            assert!(!(b.variants)().is_empty(), "{}", b.rust);
        }
    }
}
