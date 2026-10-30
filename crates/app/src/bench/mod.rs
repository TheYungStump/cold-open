//! The bench (docs/specs/bench.md): every budget in `docs/spec.md`'s table
//! measured on this machine, with the synthetic feature script, into one
//! small report with nothing personal in it.
//!
//! `coldopen-app --bench [--out FOLDER] [--quick]`, `coldopen bench`, or
//! "Measure performance…" in the app, which runs the first as a child.

pub mod machine;
pub mod measure;
pub mod report;
pub mod windowed;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use iced::Size;

use machine::{Machine, Screen};
use measure::Stats;
use report::{Budget, Info, Report, ScriptInfo, Skipped};

/// How a bench runs.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Where the two files go; the Downloads folder when `None`.
    pub out: Option<PathBuf>,
    /// Fewer repetitions, for CI's smoke tests.
    pub quick: bool,
    /// Print `progress <step>/<steps> <what>` lines, for the app's sheet.
    pub progress: bool,
}

impl Options {
    /// From the arguments after `--bench`.
    pub fn parse(args: &[String]) -> Result<Options, String> {
        let mut o = Options::default();
        let mut it = args.iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "--out" => {
                    o.out = Some(PathBuf::from(it.next().ok_or("--out needs a folder")?));
                }
                "--quick" => o.quick = true,
                "--progress" => o.progress = true,
                other => return Err(format!("unknown bench option {other}")),
            }
        }
        Ok(o)
    }
}

/// The script every bench measures.
pub const SCRIPT_NAME: &str = "synthetic feature script, seed 1";

/// The synthetic script, as Fountain.
pub fn script_text() -> String {
    coldopen_fountain::write(&coldopen_layout::synthetic::feature_script(1)).text
}

/// The steps a bench takes, for its progress lines.
const STEPS: [&str; 9] = [
    "Opening and laying out the script",
    "Typing, headless",
    "The journal",
    "Saving as Fountain, .fdx and a project",
    "Starting the app",
    "Typing in a window",
    "Scrolling the pages and the report view",
    "Memory with every page shown",
    "The other renderer",
];

/// `coldopen-app --bench …`: runs the bench, prints the summary and where
/// the files went. 0 when every budget passed, 1 when one was missed, 2
/// when the bench could not run.
pub fn main(args: &[String]) -> ExitCode {
    let opts = match Options::parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            eprintln!("usage: coldopen-app --bench [--out FOLDER] [--quick]");
            return ExitCode::from(2);
        }
    };
    let progress = opts.progress;
    let result = run(&opts, &mut |i, what| {
        if progress {
            println!("progress {}/{} {what}", i + 1, STEPS.len());
        } else {
            println!("{what}…");
        }
    });
    match result {
        Ok((report, json, txt)) => {
            println!();
            print!("{}", report.to_text());
            println!();
            println!("Report: {}", json.display());
            println!("Summary: {}", txt.display());
            if report.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("The bench could not run: {e}");
            ExitCode::from(2)
        }
    }
}

/// A folder of the bench's own, removed when it ends.
struct Work(PathBuf);

impl Work {
    fn new() -> Result<Work, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let p = std::env::temp_dir().join(format!("coldopen-bench-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|e| format!("a temporary folder: {e}"))?;
        Ok(Work(p))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs every step and writes the report. `step` hears each step as it
/// starts.
pub fn run(
    opts: &Options,
    step: &mut dyn FnMut(usize, &str),
) -> Result<(Report, PathBuf, PathBuf), String> {
    let work = Work::new()?;
    let out = opts.out.clone().unwrap_or_else(report::default_folder);
    std::fs::create_dir_all(&out).map_err(|e| format!("the report's folder: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("the app's own path: {e}"))?;
    let report = measure_all(&work.0, &exe, opts.quick, step)?;
    let (json, txt) = report
        .write(&out)
        .map_err(|e| format!("writing the report: {e}"))?;
    Ok((report, json, txt))
}

/// Repetitions: the full bench's, or `--quick`'s.
struct Reps {
    open: usize,
    keys: usize,
    edits: usize,
    saves: usize,
    cold: usize,
    frames: usize,
}

fn measure_all(
    work: &Path,
    exe: &Path,
    quick: bool,
    step: &mut dyn FnMut(usize, &str),
) -> Result<Report, String> {
    let reps = if quick {
        Reps {
            open: 3,
            keys: 20,
            edits: 100,
            saves: 5,
            cold: 2,
            frames: 30,
        }
    } else {
        Reps {
            open: 11,
            keys: 60,
            edits: 300,
            saves: 21,
            cold: 7,
            frames: 120,
        }
    };
    let source = script_text();
    let script = work.join("synthetic.fountain");
    std::fs::write(&script, &source).map_err(|e| e.to_string())?;

    step(0, STEPS[0]);
    let (open, layout, pages, blocks) = measure::open_and_layout(work, &source, reps.open)?;
    let mut r = Report::new(
        Machine::probe(),
        ScriptInfo {
            name: SCRIPT_NAME,
            pages,
            blocks,
        },
        quick,
    );
    let default = crate::files::Renderer::Auto.backends();
    r.budgets.push(Budget::ms(
        "open",
        "Open a 120-page script",
        "median",
        open.median,
        200.0,
        reps.open,
    ));
    r.budgets.push(Budget::ms(
        "repagination",
        "Full repagination",
        "median",
        layout.median,
        50.0,
        reps.open,
    ));

    step(1, STEPS[1]);
    {
        let home = work.join("headless");
        std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
        let copy = home.join("script.fountain");
        std::fs::write(&copy, &source).map_err(|e| e.to_string())?;
        match measure::open_app(&home, &copy, Size::new(1920.0, 1080.0), "tiny-skia") {
            Some(mut h) => {
                let target = (pages as u32 * 3 / 4).min(90);
                h.app.place_caret_on_page(target);
                let (update, _, total) = measure::typing(&mut h, 1.0, reps.keys);
                r.info
                    .push(info("keystroke_headless", Some("tiny-skia"), total));
                r.info.push(info("edit_and_repaginate", None, update));
                h.app.d.discard();
            }
            None => r.info.push(Info {
                id: "keystroke_headless_not_measured".into(),
                renderer: None,
                median_ms: 0.0,
                p95_ms: 0.0,
            }),
        }
    }

    step(2, STEPS[2]);
    let (app, _, fsync) = measure::journal(work, reps.edits)?;
    r.budgets.push(Budget::ms(
        "journal_write",
        "Journal write per edit",
        "p95",
        app.p95,
        1.0,
        reps.edits,
    ));
    r.info.push(info("fsync", None, fsync));

    step(3, STEPS[3]);
    for row in measure::saves(work, &source, reps.saves)? {
        r.budgets.push(Budget::ms(
            match row.id {
                "fountain" => "save_ui_fountain",
                "fdx" => "save_ui_fdx",
                _ => "save_ui_project",
            },
            match row.id {
                "fountain" => "Save on the UI thread, Fountain",
                "fdx" => "Save on the UI thread, .fdx",
                _ => "Save on the UI thread, project",
            },
            "p95",
            row.ui.p95,
            4.0,
            reps.saves,
        ));
        r.budgets.push(Budget::ms(
            match row.id {
                "fountain" => "save_whole_fountain",
                "fdx" => "save_whole_fdx",
                _ => "save_whole_project",
            },
            match row.id {
                "fountain" => "Whole save, Fountain",
                "fdx" => "Whole save, .fdx",
                _ => "Whole save, project",
            },
            "median",
            row.whole.median,
            50.0,
            reps.saves,
        ));
        r.info.push(Info {
            id: format!("save_whole_{}_p95", row.id),
            renderer: None,
            median_ms: row.whole.median,
            p95_ms: row.whole.p95,
        });
    }

    let runner = windowed::Runner {
        exe: exe.to_path_buf(),
        work: work.to_path_buf(),
        script: script.clone(),
        runs: reps.cold,
    };
    let label = |d: &windowed::Driven| d.backend.clone().unwrap_or_else(|| d.asked.clone());

    step(4, STEPS[4]);
    r.budgets.push(match runner.cold_start(default) {
        Ok(walls) => Budget::ms(
            "cold_start",
            "Cold start",
            "median",
            measure::stats(walls).median,
            1000.0,
            reps.cold,
        ),
        Err(why) => Budget::skipped("cold_start", "Cold start", "median", why),
    });

    step(5, STEPS[5]);
    let typing = runner.driven("typing", default, reps.frames);
    note_machine(&mut r.machine, &typing);
    r.budgets.push(match &typing {
        Ok(d) => Budget::ms(
            "keystroke",
            "Keystroke to screen",
            "p95",
            d.p95,
            16.0,
            reps.frames,
        )
        .on(&label(d)),
        Err(why) => Budget::skipped("keystroke", "Keystroke to screen", "p95", *why),
    });

    step(6, STEPS[6]);
    let reports = runner.driven("reports", default, reps.frames);
    note_machine(&mut r.machine, &reports);
    r.budgets.push(match &reports {
        Ok(d) => Budget::ms(
            "report_scrolling",
            "Scrolling the report view",
            "p95",
            d.p95,
            16.0,
            reps.frames,
        )
        .on(&label(d)),
        Err(why) => Budget::skipped("report_scrolling", "Scrolling the report view", "p95", *why),
    });
    if let Ok(d) = runner.driven("scrolling", default, reps.frames) {
        r.info.push(driven_info("scrolling_pages", &d, &label(&d)));
    }

    step(7, STEPS[7]);
    let memory = runner.driven("memory", default, 5000);
    note_machine(&mut r.machine, &memory);
    r.budgets.push(match memory.as_ref().map(|d| d.peak_mib) {
        Ok(Some(mib)) => Budget::mib("memory", "Memory with a feature script open", mib, 150.0)
            .on(&memory.as_ref().map(label).unwrap_or_default()),
        Ok(None) => Budget::skipped(
            "memory",
            "Memory with a feature script open",
            "peak",
            Skipped::Failed,
        ),
        Err(why) => Budget::skipped("memory", "Memory with a feature script open", "peak", *why),
    });

    step(8, STEPS[8]);
    // The renderer the app doesn't pick by default, for comparison
    // (docs/specs/bench.md §3), on its own: no fallback.
    let other = if default == "tiny-skia" {
        "wgpu"
    } else {
        "tiny-skia"
    };
    for kind in ["typing", "scrolling", "reports"] {
        match runner.driven(kind, other, reps.frames) {
            Ok(d) => {
                note_machine(&mut r.machine, &Ok(d.clone()));
                r.info
                    .push(driven_info(&format!("{kind}_windowed"), &d, &label(&d)));
            }
            Err(_) => {
                r.info.push(Info {
                    id: format!("{kind}_windowed_{other}_could_not_start"),
                    renderer: Some(other.into()),
                    median_ms: 0.0,
                    p95_ms: 0.0,
                });
                break;
            }
        }
    }
    r.finish();
    Ok(r)
}

fn info(id: &str, renderer: Option<&str>, s: Stats) -> Info {
    Info {
        id: id.into(),
        renderer: renderer.map(str::to_string),
        median_ms: (s.median * 100.0).round() / 100.0,
        p95_ms: (s.p95 * 100.0).round() / 100.0,
    }
}

fn driven_info(id: &str, d: &windowed::Driven, renderer: &str) -> Info {
    info(
        id,
        Some(renderer),
        Stats {
            median: d.median,
            p95: d.p95,
            max: d.p95,
        },
    )
}

/// What a windowed run says about the machine: its GPU and screen.
fn note_machine(m: &mut Machine, run: &Result<windowed::Driven, Skipped>) {
    let Ok(d) = run else { return };
    if let Some(a) = &d.adapter
        && d.backend.as_deref() != Some("tiny-skia")
        && !m.gpus.contains(a)
    {
        m.gpus.push(a.clone());
    }
    if m.screen.is_none()
        && let (Some((width, height)), Some(scale)) = (d.monitor, d.scale)
    {
        m.screen = Some(Screen {
            width,
            height,
            scale,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// docs/specs/bench.md §4: nothing personal in either file.
    #[test]
    fn the_report_holds_nothing_personal() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Report::new(
            Machine::probe(),
            ScriptInfo {
                name: SCRIPT_NAME,
                pages: 119,
                blocks: 3000,
            },
            true,
        );
        r.budgets.push(Budget::ms(
            "open",
            "Open a 120-page script",
            "median",
            40.0,
            200.0,
            3,
        ));
        r.budgets.push(Budget::skipped(
            "cold_start",
            "Cold start",
            "median",
            Skipped::NoWindow,
        ));
        r.finish();
        let (json, txt) = r.write(dir.path()).unwrap();
        let mut forbidden: Vec<String> = Vec::new();
        for var in [
            "HOME",
            "USERPROFILE",
            "USERNAME",
            "USER",
            "LOGNAME",
            "COMPUTERNAME",
            "HOSTNAME",
        ] {
            if let Some(v) = std::env::var_os(var) {
                let v = v.to_string_lossy().into_owned();
                if v.len() >= 3 {
                    forbidden.push(v);
                }
            }
        }
        if let Some(h) = sysinfo::System::host_name().filter(|h| h.len() >= 3) {
            forbidden.push(h);
        }
        forbidden.push(
            std::env::temp_dir()
                .to_string_lossy()
                .trim_end_matches(['/', '\\'])
                .to_string(),
        );
        forbidden.push(dir.path().to_string_lossy().into_owned());
        for line in script_text().lines().filter(|l| l.trim().len() > 12) {
            forbidden.push(line.trim().to_string());
        }
        for file in [&json, &txt] {
            let text = std::fs::read_to_string(file).unwrap();
            let lower = text.to_lowercase();
            for f in &forbidden {
                assert!(
                    !lower.contains(&f.to_lowercase()),
                    "{} holds {f:?}",
                    file.display()
                );
            }
        }
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
        assert_eq!(v["bench"], 1);
        assert_eq!(v["app"]["version"], coldopen_core::build_info::VERSION);
        assert_eq!(v["passed"], false);
        assert_eq!(v["budgets"][1]["skipped"], "no_window");
        assert!(
            json.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("coldopen-bench-")
        );
    }

    #[test]
    fn options_are_parsed() {
        let o = Options::parse(&["--quick".into(), "--out".into(), "x".into()]).unwrap();
        assert!(o.quick);
        assert_eq!(o.out, Some(PathBuf::from("x")));
        assert!(Options::parse(&["--nope".into()]).is_err());
    }
}
