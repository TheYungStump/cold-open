//! refpdf: a dev-only tool that extracts positioned text from reference
//! PDFs printed by Final Draft, so the phase 2 layout gate can compare our
//! pagination with theirs. It is never shipped. See README.md.

pub mod extract;
pub mod geom;
pub mod pageview;
pub mod reffile;
pub mod stats;
pub mod units;

/// Renders the page-view grid of an extracted PDF.
pub fn grid_text(doc: &extract::DocExtract, pitch: i64, line: i64) -> String {
    let an = reffile::analyze(doc);
    let pages: Vec<Vec<String>> = an
        .pages
        .iter()
        .map(|p| geom::grid_rows(&p.regular, pitch, line))
        .collect();
    geom::render_grid(&pages)
}
