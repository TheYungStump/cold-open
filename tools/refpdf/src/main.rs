//! Command line for refpdf. See README.md.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use refpdf::extract::{DocExtract, extract_pdf};
use refpdf::geom::{GRID_LINE, GRID_PITCH};
use refpdf::reffile::{self, Level};
use refpdf::units::parse_pt;
use refpdf::{grid_text, pageview, stats};

const USAGE: &str = "\
refpdf: positioned text from reference PDFs (dev-only, never shipped)

usage:
  refpdf extract [--out DIR] <pdf-or-dir>...
  refpdf grid [--pitch PT] [--line PT] <pdf>
  refpdf self-check [--pitch PT] [--line PT] <pdf> <pages.txt>
  refpdf stats <ref.json>...

extract   writes <stem>.ref.json next to each PDF (or into DIR) and prints checks
grid      prints the page-view grid (default pitch 7.2 pt, line 12 pt)
self-check compares the grid with a page-view file; exit 0 when identical
stats     prints numeric calibration statistics, never script text
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("refpdf: {e}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some((cmd, rest)) = args.split_first() else {
        eprint!("{USAGE}");
        return Ok(ExitCode::from(2));
    };
    match cmd.as_str() {
        "extract" => cmd_extract(rest),
        "grid" => cmd_grid(rest),
        "self-check" => cmd_self_check(rest),
        "stats" => cmd_stats(rest),
        "help" | "-h" | "--help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command {other:?}\n\n{USAGE}")),
    }
}

/// Splits `--name VALUE` / `--name=VALUE` options from positional arguments.
fn parse_opts(
    args: &[String],
    names: &[&str],
) -> Result<(BTreeMap<String, String>, Vec<String>), String> {
    let mut opts = BTreeMap::new();
    let mut pos = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--" {
            pos.extend(it.cloned());
            break;
        }
        if let Some(flag) = a.strip_prefix("--") {
            let (name, value) = match flag.split_once('=') {
                Some((n, v)) => (n.to_string(), v.to_string()),
                None => {
                    let v = it.next().ok_or_else(|| format!("--{flag} needs a value"))?;
                    (flag.to_string(), v.clone())
                }
            };
            if !names.contains(&name.as_str()) {
                return Err(format!("unknown option --{name}"));
            }
            if opts.insert(name.clone(), value).is_some() {
                return Err(format!("--{name} given twice"));
            }
        } else {
            pos.push(a.clone());
        }
    }
    Ok((opts, pos))
}

fn grid_opts(opts: &BTreeMap<String, String>) -> Result<(i64, i64), String> {
    let get = |k: &str, d: i64| {
        opts.get(k)
            .map_or(Ok(d), |v| parse_pt(v).map_err(|e| format!("--{k}: {e}")))
    };
    Ok((get("pitch", GRID_PITCH)?, get("line", GRID_LINE)?))
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn load_pdf(path: &Path) -> Result<(Vec<u8>, DocExtract), String> {
    let bytes = read(path)?;
    let doc = extract_pdf(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((bytes, doc))
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Collects PDFs below `dir` in sorted order. Symbolic links are skipped so
/// a link cycle cannot trap the walk.
fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut entries = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let ft = e.file_type()?;
        if ft.is_dir() {
            walk(&e.path(), out)?;
        } else if ft.is_file() && is_pdf(&e.path()) {
            out.push(e.path());
        }
    }
    Ok(())
}

fn cmd_extract(args: &[String]) -> Result<ExitCode, String> {
    let (opts, paths) = parse_opts(args, &["out"])?;
    if paths.is_empty() {
        return Err(format!(
            "extract needs at least one PDF or directory\n\n{USAGE}"
        ));
    }
    let out_dir = opts.get("out").map(PathBuf::from);
    if let Some(d) = &out_dir {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut pdfs = Vec::new();
    for p in &paths {
        let p = Path::new(p);
        let meta = fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
        if meta.is_dir() {
            walk(p, &mut pdfs).map_err(|e| format!("{}: {e}", p.display()))?;
        } else {
            pdfs.push(p.to_path_buf());
        }
    }
    if pdfs.is_empty() {
        return Err("no PDF files found".into());
    }
    let mut ok = true;
    let mut written = BTreeSet::new();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for pdf in &pdfs {
        let stem = pdf
            .file_stem()
            .map_or_else(|| "out".into(), |s| s.to_string_lossy().into_owned());
        let dir = out_dir
            .clone()
            .unwrap_or_else(|| pdf.parent().map_or_else(PathBuf::new, Path::to_path_buf));
        let target = dir.join(format!("{stem}.ref.json"));
        if !written.insert(target.clone()) {
            let _ = writeln!(
                out,
                "{}: skipped, {} was already written in this run",
                pdf.display(),
                target.display()
            );
            ok = false;
            continue;
        }
        let (bytes, doc) = match load_pdf(pdf) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(out, "{e}");
                ok = false;
                continue;
            }
        };
        let an = reffile::analyze(&doc);
        let r = reffile::build(reffile::sha256_hex(&bytes), &doc, &an);
        fs::write(&target, reffile::to_json(&r))
            .map_err(|e| format!("{}: {e}", target.display()))?;
        let _ = writeln!(
            out,
            "{} -> {} ({} pages)",
            pdf.display(),
            target.display(),
            r.pages.len()
        );
        for c in reffile::checks(&r, &an) {
            let tag = match c.level {
                Level::Ok => "ok  ",
                Level::Warn => "WARN",
                Level::Fail => "FAIL",
            };
            ok &= c.level != Level::Fail;
            let _ = writeln!(out, "  {tag} {}", c.msg);
        }
        for p in r.pages.iter().filter(|p| p.error.is_some()) {
            let _ = writeln!(
                out,
                "  page {}: {}",
                p.index,
                p.error.as_deref().unwrap_or_default()
            );
        }
    }
    Ok(if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn report_failed_pages(doc: &DocExtract) -> bool {
    let mut any = false;
    for p in doc.pages.iter().filter(|p| p.error.is_some()) {
        eprintln!(
            "refpdf: page {} failed: {}",
            p.index,
            p.error.as_deref().unwrap_or_default()
        );
        any = true;
    }
    any
}

fn cmd_grid(args: &[String]) -> Result<ExitCode, String> {
    let (opts, pos) = parse_opts(args, &["pitch", "line"])?;
    let [pdf] = pos.as_slice() else {
        return Err(format!("grid needs exactly one PDF\n\n{USAGE}"));
    };
    let (pitch, line) = grid_opts(&opts)?;
    let (_, doc) = load_pdf(Path::new(pdf))?;
    let text = grid_text(&doc, pitch, line);
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(if report_failed_pages(&doc) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_self_check(args: &[String]) -> Result<ExitCode, String> {
    let (opts, pos) = parse_opts(args, &["pitch", "line"])?;
    let [pdf, pages] = pos.as_slice() else {
        return Err(format!(
            "self-check needs a PDF and a page-view file\n\n{USAGE}"
        ));
    };
    let (pitch, line) = grid_opts(&opts)?;
    let (_, doc) = load_pdf(Path::new(pdf))?;
    let failed = report_failed_pages(&doc);
    let theirs =
        String::from_utf8(read(Path::new(pages))?).map_err(|_| format!("{pages}: not UTF-8"))?;
    let ours = pageview::parse(&grid_text(&doc, pitch, line));
    match pageview::compare(&ours, &pageview::parse(&theirs)) {
        None => {
            println!("identical: {} pages", ours.pages.len());
            Ok(if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            })
        }
        Some(d) => {
            let show = |s: &Option<String>| {
                s.as_ref()
                    .map_or("<none>".to_string(), |s| format!("{s:?}"))
            };
            if d.page == 0 {
                println!(
                    "first difference: before the first page header, row {}",
                    d.row
                );
            } else {
                println!("first difference: page {}, row {}", d.page, d.row);
            }
            println!("  pdf:   {}", show(&d.ours));
            println!("  pages: {}", show(&d.theirs));
            Ok(ExitCode::FAILURE)
        }
    }
}

fn cmd_stats(args: &[String]) -> Result<ExitCode, String> {
    let (_, paths) = parse_opts(args, &[])?;
    if paths.is_empty() {
        return Err(format!(
            "stats needs at least one .ref.json file\n\n{USAGE}"
        ));
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for p in &paths {
        let path = Path::new(p);
        let text = String::from_utf8(read(path)?).map_err(|_| format!("{p}: not UTF-8"))?;
        let r: reffile::RefFile = serde_json::from_str(&text).map_err(|e| format!("{p}: {e}"))?;
        if r.schema != reffile::SCHEMA {
            return Err(format!(
                "{p}: schema {} is not {}",
                r.schema,
                reffile::SCHEMA
            ));
        }
        let name = path
            .file_name()
            .map_or_else(|| p.clone(), |n| n.to_string_lossy().into_owned());
        let _ = write!(out, "{}", stats::render(&name, &stats::compute(&r)));
    }
    Ok(ExitCode::SUCCESS)
}
