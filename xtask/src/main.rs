mod contract;

use std::path::Path;
use std::process::ExitCode;

const USAGE: &str = "usage: cargo run -p xtask -- <emit-schema|diff>";

/// Emitted SDL contract, relative to the backend workspace root (the cwd of
/// `cargo run -p xtask`).
const SCHEMA_OUT: &str = "schema.graphql";
/// Node backend source tree holding the `.gql` contract files.
const NODE_SRC_DIR: &str = "../gofigeeks-gql-backend/src";

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("xtask: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let out = Path::new(SCHEMA_OUT);
    match std::env::args().nth(1).as_deref() {
        Some("emit-schema") => {
            contract::emit_schema(out)?;
            println!("wrote {SCHEMA_OUT}");
            Ok(ExitCode::SUCCESS)
        }
        Some("diff") => {
            let report = contract::diff(out, Path::new(NODE_SRC_DIR))?;
            Ok(print_report(report))
        }
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`");
            eprintln!("{USAGE}");
            Ok(ExitCode::from(2))
        }
        None => {
            eprintln!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

fn print_report(report: contract::Report) -> ExitCode {
    for note in &report.info {
        println!("  info: {note}");
    }
    if let Some(stale) = &report.stale {
        println!("  stale: {stale}");
    }
    for issue in &report.breaking {
        println!("  BREAKING: {issue}");
    }

    if report.breaking.is_empty() && report.stale.is_none() {
        println!("contract diff clean: emitted SDL matches the Node .gql contract");
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "contract diff failed: {} breaking change(s){}",
            report.breaking.len(),
            if report.stale.is_some() {
                ", committed schema stale"
            } else {
                ""
            }
        );
        ExitCode::FAILURE
    }
}
