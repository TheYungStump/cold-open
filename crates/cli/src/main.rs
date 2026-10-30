//! `coldopen`: the Cold Open command-line tool.
//!
//! Every subcommand works by sending commands to the core dispatcher, the
//! same way the desktop app and agents do (docs/specs/commands.md).

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use coldopen_core::{Dispatcher, Options};
use serde_json::{Value, json};

mod report;

#[derive(Parser)]
#[command(
    name = "coldopen",
    version = coldopen_core::build_info::VERSION_LINE,
    about = "Cold Open: a screenwriting app you can drive from the command line"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Fountain,
    Fdx,
    Json,
}

#[derive(Subcommand)]
enum Cmd {
    /// Convert a script to Fountain, .fdx or JSON.
    Convert {
        /// The script to read (.fountain, .spmd, .txt or .fdx).
        input: PathBuf,
        /// Where to write, or - for standard output.
        output: String,
        /// Output format. Defaults to the output file's extension, or Fountain.
        #[arg(long, value_enum)]
        to: Option<Format>,
        /// Write even if the format cannot hold everything in the script.
        #[arg(long)]
        allow_lossy: bool,
    },
    /// Repair a damaged .fdx file into a new one, and print what was
    /// repaired. The input is never written.
    Repair {
        /// The .fdx file to repair.
        input: PathBuf,
        /// Where to write the repaired copy (.fdx).
        output: PathBuf,
        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
        /// Also write the report to this file.
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Export a script as a PDF.
    Export {
        /// The script to read (.fountain, .spmd, .txt or .fdx).
        input: PathBuf,
        /// The PDF to write.
        output: PathBuf,
        /// Print this text diagonally across every page.
        #[arg(long)]
        watermark: Option<String>,
        /// Print scene numbers beside the scene headings.
        #[arg(long)]
        scene_numbers: bool,
        /// Leave out the title page.
        #[arg(long)]
        no_title_page: bool,
    },
    /// Print the pages of a script as plain text, the way they print.
    Pages {
        /// The script to read.
        file: PathBuf,
    },
    /// Print a report: scenes, characters or locations
    /// (docs/specs/reports.md).
    Report {
        /// Which report.
        #[arg(value_enum)]
        kind: report::Kind,
        /// The script to read.
        file: PathBuf,
        /// Text for people, the command's JSON, or CSV (RFC 4180).
        #[arg(long, value_enum, default_value_t = report::Form::Text)]
        format: report::Form,
    },
    /// Compare two drafts, in any of the three formats, scene by scene
    /// (docs/specs/diff.md). Prints a summary; exits 0 whether or not they
    /// differ.
    Diff {
        /// The first draft.
        a: PathBuf,
        /// The second draft.
        b: PathBuf,
        /// Print the comparison as JSON.
        #[arg(long)]
        json: bool,
        /// Also write the second draft with the changes marked to this PDF.
        #[arg(long)]
        pdf: Option<PathBuf>,
        /// Replace the PDF if it exists.
        #[arg(long)]
        overwrite: bool,
    },
    /// List the scenes in a script.
    Scenes {
        /// The script to read.
        file: PathBuf,
        /// Print the raw scene.list result as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Run a batch of commands from a JSON file (an array of requests, or
    /// one request per line). Prints one response per line.
    Run {
        /// The file of command requests, or - for standard input.
        edits: String,
        /// Open this script before running the commands.
        #[arg(long)]
        doc: Option<PathBuf>,
        /// Save the script after the commands succeed.
        #[arg(long)]
        save: bool,
        /// Keep going after a command fails.
        #[arg(long)]
        keep_going: bool,
        /// The name your tracked changes carry (default "Writer").
        #[arg(long)]
        author: Option<String>,
    },
    /// Print the licenses of Cold Open and of the fonts it bundles.
    Licenses,
    /// Read JSON requests on standard input, one per line, and answer each
    /// on standard output.
    Serve {
        /// Use standard input and output (the only transport so far).
        #[arg(long)]
        stdio: bool,
        /// The name your tracked changes carry (default "Writer").
        #[arg(long)]
        author: Option<String>,
    },
    /// Serve the Model Context Protocol on standard input and output, so an
    /// AI agent can read and edit scripts. When the Cold Open app has the
    /// script open, the agent's edits go into the app, live and undoable.
    Mcp {
        /// A script to open, or attach to, before the first request.
        #[arg(long)]
        doc: Option<PathBuf>,
        /// Never attach to the app; always open the file in this process.
        #[arg(long)]
        no_attach: bool,
        /// Do not record the agent's edits as tracked changes in scripts this
        /// server opens itself. When the script is open in the app, the
        /// app's setting decides.
        #[arg(long)]
        untracked: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.cmd {
        Cmd::Convert {
            input,
            output,
            to,
            allow_lossy,
        } => convert(&input, &output, to, allow_lossy),
        Cmd::Export {
            input,
            output,
            watermark,
            scene_numbers,
            no_title_page,
        } => export(&input, &output, watermark, scene_numbers, no_title_page),
        Cmd::Repair {
            input,
            output,
            json,
            report,
        } => repair(&input, &output, json, report.as_deref()),
        Cmd::Pages { file } => pages(&file),
        Cmd::Scenes { file, json } => scenes(&file, json),
        Cmd::Report { kind, file, format } => run_report(kind, &file, format),
        Cmd::Diff {
            a,
            b,
            json,
            pdf,
            overwrite,
        } => run_diff(&a, &b, json, pdf.as_deref(), overwrite),
        Cmd::Licenses => licenses(),
        Cmd::Run {
            edits,
            doc,
            save,
            keep_going,
            author,
        } => run(&edits, doc.as_deref(), save, keep_going, author),
        Cmd::Mcp {
            doc,
            no_attach,
            untracked,
        } => coldopen_mcp::run_stdio(coldopen_mcp::McpOptions {
            doc,
            no_attach,
            run_dir: coldopen_link::run_dir(),
            untracked,
        })
        .map(|()| ExitCode::SUCCESS),
        Cmd::Serve { stdio, author } => {
            if stdio {
                serve_stdio(author)
            } else {
                Err("choose a transport: coldopen serve --stdio".into())
            }
        }
    };
    match result {
        Ok(code) => code,
        Err(message) => {
            eprintln!("coldopen: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Sends one command and returns its result, or the error as a message.
fn call(d: &mut Dispatcher, command: &str, params: Value) -> Result<Value, String> {
    let r = d.handle(&json!({ "v": 1, "command": command, "params": params }));
    if r["ok"] == true {
        Ok(r["result"].clone())
    } else {
        let e = &r["error"];
        let mut msg = format!(
            "{} ({})",
            e["message"].as_str().unwrap_or("failed"),
            e["code"].as_str().unwrap_or("error")
        );
        if let Some(losses) = e["details"]["losses"].as_array() {
            for l in losses {
                msg.push_str(&format!(
                    "\n  {}: {}",
                    l["id"].as_str().unwrap_or("title page"),
                    l["message"].as_str().unwrap_or("")
                ));
            }
        }
        Err(msg)
    }
}

/// A dispatcher for read-only work: no journal next to the input.
fn reader() -> Dispatcher {
    Dispatcher::new(Options {
        journal: false,
        fsync: false,
    })
}

fn path_str(p: &Path) -> String {
    p.display().to_string()
}

fn print_warnings(result: &Value) {
    if let Some(ws) = result["warnings"].as_array() {
        for w in ws {
            match w["line"].as_u64() {
                Some(line) => eprintln!(
                    "coldopen: warning: line {line}: {}",
                    w["message"].as_str().unwrap_or("")
                ),
                None => eprintln!("coldopen: warning: {}", w["message"].as_str().unwrap_or("")),
            }
        }
    }
}

fn convert(
    input: &Path,
    output: &str,
    to: Option<Format>,
    allow_lossy: bool,
) -> Result<ExitCode, String> {
    if Path::new(output)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err(
            "convert writes Fountain, .fdx or JSON; use `coldopen export` to make a PDF".into(),
        );
    }
    let format = to.unwrap_or_else(|| {
        let ext = Path::new(output)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("json") => Format::Json,
            Some("fdx") => Format::Fdx,
            _ => Format::Fountain,
        }
    });
    let mut d = reader();
    let opened = call(&mut d, "doc.open", json!({ "path": path_str(input) }))?;
    print_warnings(&opened);
    let name = match format {
        Format::Fountain => "fountain",
        Format::Fdx => "fdx",
        Format::Json => "json",
    };
    let exported = call(&mut d, "doc.export", json!({ "format": name }))?;
    let losses = exported["losses"].as_array().cloned().unwrap_or_default();
    if !losses.is_empty() {
        for l in &losses {
            eprintln!(
                "coldopen: {}: {}",
                l["id"].as_str().unwrap_or("title page"),
                l["message"].as_str().unwrap_or("")
            );
        }
        if !allow_lossy {
            let what = match format {
                Format::Fdx => "An .fdx file",
                _ => "Fountain",
            };
            return Err(format!(
                "{what} cannot hold everything in this script; nothing was written (use --allow-lossy to write anyway)"
            ));
        }
    }
    let text = exported["text"].as_str().unwrap_or_default();
    if output == "-" {
        let mut out = io::stdout().lock();
        out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    } else {
        let path = Path::new(output);
        coldopen_core::io::atomic_write(path, text.as_bytes())
            .map_err(|e| format!("{output}: {e}"))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `coldopen repair` (docs/specs/fdx.md §17): `file.repair`, then the
/// report as text or JSON.
fn repair(
    input: &Path,
    output: &Path,
    json_out: bool,
    report: Option<&Path>,
) -> Result<ExitCode, String> {
    let mut d = reader();
    // The output was named on the command line, so it is replaced, as
    // `convert` and `export` replace theirs; the input never is.
    let result = call(
        &mut d,
        "file.repair",
        json!({ "path": path_str(input), "out": path_str(output), "overwrite": true }),
    )?;
    let issues = result["issues"].as_array().cloned().unwrap_or_default();
    let text = if json_out {
        let v = json!({ "repaired": result["repaired"], "issues": issues });
        serde_json::to_string_pretty(&v).map_err(|e| e.to_string())? + "\n"
    } else {
        report_text(&result, &issues)
    };
    print!("{text}");
    if let Some(path) = report {
        coldopen_core::io::atomic_write(path, text.as_bytes())
            .map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// The repair report for people (docs/specs/fdx.md §9): repairs one per
/// line, then a count of what was kept or noted.
fn report_text(result: &Value, issues: &[Value]) -> String {
    let mut out = String::new();
    let class = |i: &Value| i["class"].as_str().unwrap_or("").to_string();
    let repairs: Vec<&Value> = issues
        .iter()
        .filter(|i| matches!(class(i).as_str(), "fixed" | "dropped"))
        .collect();
    if result["repaired"] == true {
        out.push_str(&format!(
            "Repaired, with {} {}:\n",
            repairs.len(),
            if repairs.len() == 1 {
                "change"
            } else {
                "changes"
            }
        ));
    } else {
        out.push_str("Nothing needed repair; the copy is the file as it was.\n");
    }
    for i in &repairs {
        let at = match (
            i["line"].as_u64(),
            i["column"].as_u64(),
            i["paragraph"].as_u64(),
        ) {
            (Some(l), Some(c), _) => format!("line {l}, column {c}: "),
            (_, _, Some(p)) => format!("paragraph {p}: "),
            _ => String::new(),
        };
        out.push_str(&format!(
            "  {:<8} {at}{} ({})\n",
            class(i),
            i["message"].as_str().unwrap_or(""),
            i["code"].as_str().unwrap_or("")
        ));
    }
    let kept = issues.iter().filter(|i| class(i) == "kept").count();
    let info = issues.iter().filter(|i| class(i) == "info").count();
    if kept + info > 0 {
        out.push_str(&format!(
            "Also: {kept} kept as they are (Cold Open does not use them), {info} noted. Run with --json for every item.\n"
        ));
    }
    out
}

fn export(
    input: &Path,
    output: &Path,
    watermark: Option<String>,
    scene_numbers: bool,
    no_title_page: bool,
) -> Result<ExitCode, String> {
    let mut d = reader();
    let opened = call(&mut d, "doc.open", json!({ "path": path_str(input) }))?;
    print_warnings(&opened);
    // The output was named on the command line, so it is replaced, as
    // `convert` replaces its output.
    let mut params = json!({
        "path": path_str(output),
        "scene_numbers": scene_numbers,
        "title_page": !no_title_page,
        "overwrite": true,
    });
    if let Some(w) = watermark {
        params["watermark"] = json!(w);
    }
    let result = call(&mut d, "export.pdf", params)?;
    if let Some(ws) = result["warnings"].as_array() {
        for w in ws {
            let chars = w["chars"]
                .as_str()
                .map(|c| format!(": {c}"))
                .unwrap_or_default();
            eprintln!(
                "coldopen: warning: {}{chars}",
                w["message"].as_str().unwrap_or("")
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn pages(file: &Path) -> Result<ExitCode, String> {
    let mut d = reader();
    let opened = call(&mut d, "doc.open", json!({ "path": path_str(file) }))?;
    print_warnings(&opened);
    let result = call(&mut d, "doc.export", json!({ "format": "pages" }))?;
    let mut out = io::stdout().lock();
    out.write_all(result["text"].as_str().unwrap_or_default().as_bytes())
        .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

/// A length in eighths of a page, the way scripts write it: `3/8`, `1`,
/// `1 2/8`.
fn eighths(n: u64) -> String {
    match (n / 8, n % 8) {
        (0, r) => format!("{r}/8"),
        (w, 0) => format!("{w}"),
        (w, r) => format!("{w} {r}/8"),
    }
}

fn licenses() -> Result<ExitCode, String> {
    let mut out = io::stdout().lock();
    let text = format!(
        "Cold Open is free software, licensed under the GNU General Public License,\n\
         version 3 or later. Its format and layout libraries are also available\n\
         under the MIT or Apache-2.0 licenses. See the repository for the full texts.\n\
         \n\
         Cold Open bundles the Courier Prime fonts, licensed as follows.\n\
         \n\
         {}\n\
         The desktop app also bundles the Atkinson Hyperlegible Next fonts,\n\
         subset to Latin, licensed as follows.\n\
         \n\
         {}",
        coldopen_fonts::LICENSE,
        coldopen_fonts::UI_LICENSE
    );
    out.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn scenes(file: &Path, as_json: bool) -> Result<ExitCode, String> {
    let mut d = reader();
    let opened = call(&mut d, "doc.open", json!({ "path": path_str(file) }))?;
    print_warnings(&opened);
    let result = call(&mut d, "scene.list", json!({}))?;
    let mut out = io::stdout().lock();
    if as_json {
        writeln!(out, "{result}").map_err(|e| e.to_string())?;
        return Ok(ExitCode::SUCCESS);
    }
    let scenes = result["scenes"].as_array().cloned().unwrap_or_default();
    let num_width = scenes
        .iter()
        .map(|s| s["number"].as_str().map_or(1, str::len))
        .max()
        .unwrap_or(1)
        .max(1);
    for s in &scenes {
        let page = s["page"]
            .as_u64()
            .map_or_else(|| "-".to_string(), |p| format!("p. {p}"));
        let line = format!(
            "{:>4}  {:<nw$}  {:>6}  {:>6}  {}  ({} blocks, {} words)",
            s["index"].as_u64().unwrap_or(0),
            s["number"].as_str().unwrap_or("-"),
            page,
            eighths(s["eighths"].as_u64().unwrap_or(0)),
            s["heading"].as_str().unwrap_or(""),
            s["blocks"].as_u64().unwrap_or(0),
            s["words"].as_u64().unwrap_or(0),
            nw = num_width,
        );
        writeln!(out, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(ExitCode::SUCCESS)
}

fn run_report(kind: report::Kind, file: &Path, form: report::Form) -> Result<ExitCode, String> {
    let mut d = reader();
    let opened = call(&mut d, "doc.open", json!({ "path": path_str(file) }))?;
    print_warnings(&opened);
    let result = call(&mut d, kind.command(), json!({}))?;
    // Scenes by their number in the script, for people and spreadsheets.
    let index = call(&mut d, "scene.list", json!({}))?["scenes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|s| Some((s["id"].as_str()?.to_string(), s["index"].as_u64()?)))
        .collect();
    let text = report::render(kind, form, &result, &index);
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

fn run_diff(
    a: &Path,
    b: &Path,
    as_json: bool,
    pdf: Option<&Path>,
    overwrite: bool,
) -> Result<ExitCode, String> {
    let mut d = reader();
    let mut params = json!({ "a": path_str(a), "b": path_str(b), "overwrite": overwrite });
    if let Some(p) = pdf {
        params["pdf"] = json!(path_str(p));
    }
    let result = call(&mut d, "diff", params)?;
    let text = if as_json {
        let mut v = result.clone();
        if let Value::Object(m) = &mut v {
            m.remove("text");
        }
        format!("{v}\n")
    } else {
        result["text"].as_str().unwrap_or_default().to_string()
    };
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(ExitCode::SUCCESS)
}

/// Reads requests from a JSON array or from one JSON value per line.
fn read_requests(source: &str) -> Result<Vec<Value>, String> {
    let text = if source == "-" {
        io::read_to_string(io::stdin()).map_err(|e| e.to_string())?
    } else {
        std::fs::read_to_string(source).map_err(|e| format!("{source}: {e}"))?
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('[') {
        return serde_json::from_str::<Vec<Value>>(trimmed)
            .map_err(|e| format!("{source}: not a JSON array of requests: {e}"));
    }
    trimmed
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, l)| {
            serde_json::from_str(l).map_err(|e| format!("{source}:{}: not valid JSON: {e}", n + 1))
        })
        .collect()
}

fn run(
    source: &str,
    doc: Option<&Path>,
    save: bool,
    keep_going: bool,
    author: Option<String>,
) -> Result<ExitCode, String> {
    let mut requests = read_requests(source)?;
    if let Some(p) = doc {
        requests.insert(
            0,
            json!({ "id": "--doc", "command": "doc.open", "params": { "path": path_str(p) } }),
        );
    }
    if save {
        requests.push(json!({ "id": "--save", "command": "doc.save" }));
    }

    let mut d = Dispatcher::new(Options::default());
    d.set_writer_name(author);
    let mut out = io::stdout().lock();
    let mut failed = false;
    for req in &requests {
        let response = d.handle(req);
        writeln!(out, "{}", coldopen_core::to_line(&response)).map_err(|e| e.to_string())?;
        if response["ok"] != true {
            failed = true;
            if !keep_going {
                break;
            }
        }
    }
    out.flush().map_err(|e| e.to_string())?;

    // A batch can be rerun from its file, so its own unsaved changes are
    // not kept. Work recovered from a crash journal was never the batch's
    // to throw away (docs/specs/journal.md, "Closing").
    let (dirty, recovered) = d.session().map_or((false, None), |s| {
        (s.dirty, s.recovered.map(|r| r.rev == s.rev))
    });
    match (dirty, recovered) {
        (true, Some(true)) => {
            eprintln!(
                "coldopen: the document holds unsaved changes recovered from its crash journal; \
                 they were left in the journal, so opening the file recovers them again"
            );
            d.shutdown();
        }
        (true, Some(false)) => match d.discard() {
            Some(kept) => eprintln!(
                "coldopen: the batch's unsaved changes were discarded, with the unsaved changes \
                 recovered from the crash journal; both were kept in {}",
                kept.display()
            ),
            None => eprintln!(
                "coldopen: the batch's unsaved changes could not be set aside, so the crash \
                 journal was left in place; opening the file recovers them"
            ),
        },
        (true, None) => {
            eprintln!(
                "coldopen: unsaved changes were discarded; use --save or end the batch with doc.save"
            );
            d.discard_rerunnable();
        }
        (false, _) => d.shutdown(),
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn serve_stdio(author: Option<String>) -> Result<ExitCode, String> {
    let mut d = Dispatcher::new(Options::default());
    d.set_writer_name(author);
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut out = io::stdout().lock();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let n = input
            .read_until(b'\n', &mut buf)
            .map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let response = match std::str::from_utf8(&buf) {
            Ok(line) if line.trim().is_empty() => continue,
            // Windows PowerShell starts what it pipes with a byte-order
            // mark; it is not part of the request.
            Ok(line) => d.handle_line(
                line.trim_start_matches('\u{feff}')
                    .trim_end_matches(['\r', '\n']),
            ),
            Err(_) => json!({
                "v": 1, "id": null, "ok": false,
                "error": { "code": "invalid_request", "message": "a request line must be UTF-8" }
            })
            .to_string(),
        };
        writeln!(out, "{response}").map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    // End of input: unsaved work stays in the crash journal for next time.
    if let Some(s) = d.session().filter(|s| s.dirty) {
        if s.journal.is_some() {
            eprintln!(
                "coldopen: unsaved changes are kept in the crash journal; open the file again to recover them"
            );
        } else {
            eprintln!(
                "coldopen: the untitled document's unsaved changes were not kept; save it with a path to protect it"
            );
        }
    }
    d.shutdown();
    Ok(ExitCode::SUCCESS)
}
