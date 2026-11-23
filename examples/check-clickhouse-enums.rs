//! Checks the Rust enums against the `Enum8`/`Enum16` columns they decode.
//!
//! `cargo run --example check-clickhouse-enums --features product-<p> -- <clickhouse-url>
//! [database]`. The database defaults to `<product><YYMM>` of this crate's version.

use std::process::ExitCode;

use clickhouse::Client;
use platform_api::{
    datasource::enum_check::{bindings, mismatches, parse_enum_type},
    product::{CurrentProduct, Product},
};

#[tokio::main]
async fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(url) = args.next() else {
        eprintln!("usage: check-clickhouse-enums <clickhouse-url> [database]");
        return ExitCode::from(2);
    };
    let database = args.next().unwrap_or_else(|| {
        let release: String = env!("CARGO_PKG_VERSION").split('.').take(2).collect();
        format!("{}{release}", <CurrentProduct as Product>::NAME)
    });
    let client = Client::default().with_url(url).with_database(&database);

    let bindings = bindings();
    let mut failed = false;
    for b in &bindings {
        let where_ = format!("{} {}.{}", b.rust, b.table, b.expr);
        let sql = format!("DESCRIBE (SELECT {} FROM {})", b.expr, b.table);
        let described = match client.query(&sql).fetch_bytes("TSVRaw") {
            Ok(mut cursor) => cursor.collect().await,
            Err(e) => Err(e),
        };
        let type_name = match described {
            Ok(bytes) => String::from_utf8_lossy(&bytes)
                .lines()
                .next()
                .and_then(|line| line.split('\t').nth(1))
                .map(str::to_string),
            Err(e) => {
                println!("{where_}: {e}");
                failed = true;
                continue;
            }
        };
        let db = match type_name.as_deref().map(parse_enum_type) {
            Some(Ok(values)) => values,
            Some(Err(e)) => {
                println!("{where_}: {e}");
                failed = true;
                continue;
            }
            None => {
                println!("{where_}: DESCRIBE returned no type");
                failed = true;
                continue;
            }
        };
        for m in mismatches(&(b.variants)(), &db) {
            println!("{where_}: {m}");
            failed = true;
        }
    }
    if failed {
        return ExitCode::FAILURE;
    }
    println!("{} enums checked", bindings.len());
    ExitCode::SUCCESS
}
