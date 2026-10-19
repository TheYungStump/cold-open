//! Where a bundle save's time goes (docs/specs/bundle.md, docs/spec.md's
//! "Autosave and journal write: under 10 ms"): the JSON, the readable
//! Fountain copy, deflate at each level, the archive, the atomic write and
//! the journal's new header, each timed alone. Works on a copy of FILE,
//! converted to a bundle with tracked changes on every fourth scene, in a
//! temporary folder.
//!
//! ```text
//! cargo run --release -p coldopen-core --example save_profile -- FILE [RUNS]
//! ```
//!
//! Prints numbers only, never script text.

use std::time::Instant;

use coldopen_core::io::SourceStyle;
use coldopen_core::{Author, Command, Dispatcher, FormatId, Options};
use serde_json::{Value, json};

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// The median time of `f`, in ms, over `runs` runs, with its last result.
fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut v = Vec::new();
    let mut out = None;
    for _ in 0..runs {
        let t = Instant::now();
        out = Some(f());
        v.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    (median(v), out.unwrap())
}

fn run(d: &mut Dispatcher, by: &Author, c: &str, p: Value) -> Value {
    d.execute_as(Command::from_parts(c, p).unwrap(), by)
        .unwrap_or_else(|e| panic!("{c}: {}", e.message))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(file) = args.next() else {
        eprintln!("usage: save_profile FILE [RUNS]");
        std::process::exit(2);
    };
    let runs: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(21);
    let src = std::path::PathBuf::from(file);
    let work = tempfile::tempdir().unwrap();
    let copy = work.path().join(src.file_name().unwrap());
    std::fs::copy(&src, &copy).unwrap();

    // The document: converted, with tracked edits on every fourth scene.
    let writer = Author::writer();
    let agent = Author::agent("measure", None);
    let mut d = Dispatcher::new(Options::default());
    run(&mut d, &writer, "doc.open", json!({ "path": copy }));
    run(&mut d, &writer, "changes.track", json!({ "writer": true }));
    let blocks = run(&mut d, &writer, "doc.get", json!({}))["blocks"]
        .as_array()
        .unwrap()
        .clone();
    let mut scene = 0usize;
    for (i, b) in blocks.iter().enumerate() {
        if b["type"] != "scene_heading" {
            continue;
        }
        if scene.is_multiple_of(4) {
            run(
                &mut d,
                &writer,
                "block.insert",
                json!({ "after": b["id"], "type": "action", "text": "A line the writer added." }),
            );
            if let Some(a) = blocks[i + 1..]
                .iter()
                .take_while(|x| x["type"] != "scene_heading")
                .find(|x| x["type"] == "action" && x["text"].as_str().unwrap_or("").len() > 6)
            {
                run(
                    &mut d,
                    &agent,
                    "text.delete",
                    json!({ "id": a["id"], "from": 0, "to": 6 }),
                );
            }
        }
        scene += 1;
    }
    let doc = d.session().unwrap().doc.clone();
    drop(d);
    println!(
        "{} blocks, {} pending changes, median of {runs} runs",
        doc.blocks.len(),
        doc.pending().len()
    );

    let (t_doc, doc_json) = time(runs, || serde_json::to_vec(&doc).unwrap());
    let (t_changes, changes_json) =
        time(runs, || serde_json::to_vec(&doc.changes.to_file()).unwrap());
    let (t_script, script) = time(runs, || {
        FormatId::Fountain
            .get()
            .write(&doc, SourceStyle::default())
            .bytes
    });
    println!(
        "document.json {} bytes {t_doc:.2} ms; changes.json {} bytes {t_changes:.2} ms; script.fountain {} bytes {t_script:.2} ms",
        doc_json.len(),
        changes_json.len(),
        script.len()
    );
    // Inside the Fountain copy.
    let (t_accept, accepted) = time(runs, || doc.accepted());
    let (t_to_script, script_ast) = time(runs, || accepted.to_script());
    let (t_fountain, _) = time(runs, || coldopen_fountain::write(&script_ast).text.len());
    let w = coldopen_fountain::write(&script_ast);
    println!(
        "  the writer reports {} losses and {} normalizations",
        w.losses.len(),
        w.normalized.len()
    );
    let text = w.text;
    let (t_parse, _) = time(runs, || {
        coldopen_fountain::parse(&text).script.elements.len()
    });
    let (t_norm, _) = time(runs, || {
        coldopen_fountain::normalize_with_source_map(&script_ast)
            .0
            .elements
            .len()
    });
    println!(
        "  inside the Fountain writer: normal form {t_norm:.2} ms; reading the output back to check it {t_parse:.2} ms"
    );
    let (t_fast, fast) = time(runs, || {
        let (s, _) = (doc.accepted().to_script(), ());
        coldopen_fountain::write_text(&s)
    });
    println!(
        "  the writer's quick path (text only, as the bundle writes it now): {t_fast:.2} ms, same text: {}",
        fast == coldopen_fountain::write(&doc.accepted().to_script()).text
    );
    let mut unstyled = script_ast.clone();
    let mut styled_lines = 0;
    for e in &mut unstyled.elements {
        let runs: Vec<coldopen_screenplay::Run> = e.text.runs().to_vec();
        if runs.iter().any(|r| match r {
            coldopen_screenplay::Run::Text { style, .. } => !style.is_plain(),
            _ => false,
        }) {
            styled_lines += 1;
        }
        e.text = coldopen_screenplay::RichText::plain(e.text.plain_text());
    }
    let (t_unstyled, _) = time(runs, || coldopen_fountain::write_text(&unstyled).len());
    let specials = text
        .lines()
        .filter(|l| l.contains(['*', '_', '\\', '[', '/']))
        .count();
    println!(
        "  {styled_lines} styled elements, {specials} written lines with * _ \\ [ or /; the quick path with every style taken off: {t_unstyled:.2} ms"
    );
    let plain = doc.accepted();
    let (t_plain, _) = time(runs, || {
        FormatId::Fountain
            .get()
            .write(&plain, SourceStyle::default())
            .bytes
            .len()
    });
    println!(
        "  inside it: accept the pending changes {t_accept:.2} ms, to_script {t_to_script:.2} ms, Fountain writer {t_fountain:.2} ms; the same document with no changes pending {t_plain:.2} ms"
    );
    let (t_crc, _) = time(runs, || {
        rawzip::crc32(&doc_json) ^ rawzip::crc32(&changes_json) ^ rawzip::crc32(&script)
    });
    println!("CRC-32 of all entries {t_crc:.2} ms");
    for level in [1u8, 2, 3, 4, 6] {
        let (t, packed) = time(runs, || {
            [&doc_json, &changes_json, &script]
                .map(|e| miniz_oxide::deflate::compress_to_vec(e, level).len())
        });
        println!(
            "deflate level {level}: {} + {} + {} bytes, {t:.2} ms",
            packed[0], packed[1], packed[2]
        );
    }
    let (t_bundle, bytes) = time(runs, || {
        FormatId::Bundle
            .get()
            .write(&doc, SourceStyle::default())
            .bytes
    });
    println!(
        "the whole bundle as written today: {} bytes, {t_bundle:.2} ms",
        bytes.len()
    );

    let target = work.path().join("profile.coldopen");
    let (t_write, _) = time(runs, || {
        coldopen_core::io::atomic_write(&target, &bytes).unwrap()
    });
    println!("atomic write of those bytes (fsync, rename): {t_write:.2} ms");

    // The archive end to end at each setting: the JSON, the readable copy
    // by the quick path, compression, the archive, then the atomic write.
    let build = |level: Option<u8>| {
        let doc_json = serde_json::to_vec(&doc).unwrap();
        let changes_json = serde_json::to_vec(&doc.changes.to_file()).unwrap();
        let script = coldopen_fountain::write_text(&doc.accepted().to_script()).into_bytes();
        let mut out = Vec::new();
        let mut archive = rawzip::ZipArchiveWriter::new(&mut out);
        for (name, data) in [
            ("document.json", &doc_json),
            ("changes.json", &changes_json),
            ("script.fountain", &script),
        ] {
            use std::io::Write;
            let method = match level {
                Some(_) => rawzip::CompressionMethod::DEFLATE,
                None => rawzip::CompressionMethod::STORE,
            };
            let (mut entry, _) = archive
                .new_file(name)
                .compression_method(method)
                .start()
                .unwrap();
            match level {
                Some(l) => entry
                    .write_all(&miniz_oxide::deflate::compress_to_vec(data, l))
                    .unwrap(),
                None => entry.write_all(data).unwrap(),
            }
            entry
                .finish(rawzip::DataDescriptorOutput::new(
                    rawzip::crc32(data),
                    data.len() as u64,
                ))
                .unwrap();
        }
        archive.finish().unwrap();
        out
    };
    for level in [None, Some(1u8), Some(2), Some(3), Some(6)] {
        let (t_build, bytes) = time(runs, || build(level));
        let target = work.path().join("level.coldopen");
        let (t_w, _) = time(runs, || {
            coldopen_core::io::atomic_write(&target, &bytes).unwrap()
        });
        println!(
            "{}: {} bytes; archive {t_build:.2} ms + atomic write {t_w:.2} ms = {:.2} ms",
            level.map_or("stored".to_string(), |l| format!("deflate level {l}")),
            bytes.len(),
            t_build + t_w
        );
    }

    // The journal's header after a save: the document again, as JSON, then
    // written and flushed.
    let header_path = work.path().join("header.json");
    let (t_header_json, header) = time(runs, || serde_json::to_vec(&doc).unwrap());
    let (t_header_write, _) = time(runs, || {
        use std::io::Write;
        let mut f = std::fs::File::create(&header_path).unwrap();
        f.write_all(&header).unwrap();
        f.sync_all().unwrap();
    });
    println!(
        "journal header: {} bytes, JSON {t_header_json:.2} ms, write and fsync {t_header_write:.2} ms",
        header.len()
    );
}
