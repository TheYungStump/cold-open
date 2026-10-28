//! `coldopen report scenes|characters|locations FILE [--format text|json|csv]`
//! (docs/specs/reports.md §5). The rows and both printed forms come from
//! core, which the desktop app's report view shares.

use std::collections::BTreeMap;

use coldopen_core::report;
use serde_json::Value;

/// Which report.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Kind {
    Scenes,
    Characters,
    Locations,
}

impl Kind {
    pub fn core(self) -> report::Kind {
        match self {
            Kind::Scenes => report::Kind::Scenes,
            Kind::Characters => report::Kind::Characters,
            Kind::Locations => report::Kind::Locations,
        }
    }

    pub fn command(self) -> &'static str {
        self.core().command()
    }
}

/// How a report is printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Form {
    #[default]
    Text,
    Json,
    Csv,
}

/// The report printed in `form`. `index` maps each scene's id to its number
/// in the script.
pub fn render(kind: Kind, form: Form, result: &Value, index: &BTreeMap<String, u64>) -> String {
    match form {
        Form::Json => format!("{result}\n"),
        Form::Csv => report::csv(&report::table(kind.core(), result, index)),
        Form::Text => report::text(&report::table(kind.core(), result, index)),
    }
}
