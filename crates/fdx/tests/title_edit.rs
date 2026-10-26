//! Edited title lines written back (docs/specs/fdx.md §13.5): the lines
//! left alone stay as the file had them, and edited lines carry what their
//! paragraphs held. Run on our fixtures and, when it is present, on the
//! private corpus; `COLDOPEN_REQUIRE_CORPUS=1` makes a missing corpus fail.
//! Nothing printed here holds script text.

use std::path::{Path, PathBuf};

use coldopen_fdx::{Doc, fuzzing, read};
use coldopen_screenplay::{RichText, Run, Style};

fn corpus_dir() -> PathBuf {
    match std::env::var_os("COLDOPEN_CORPUS") {
        Some(p) => PathBuf::from(p),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus"),
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fdx")
}

fn fdx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            fdx_files(&p, out);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fdx")) {
            out.push(p);
        }
    }
}

/// Edits as `fuzzing::title_edit` reads them: change a line's text, remove
/// one, insert one, change one's alignment and space above, and mixes.
const EDITS: [&[u8]; 8] = [
    &[0, 0],
    &[0, 3],
    &[1, 1],
    &[2, 0],
    &[2, 255],
    &[3, 2],
    &[0, 1, 1, 2, 2, 5, 3, 7],
    &[1, 0, 1, 0, 1, 0, 2, 1, 0, 9],
];

fn check_all(files: &[PathBuf]) -> usize {
    let mut with_title = 0;
    for p in files {
        let data = std::fs::read(p).unwrap();
        let name = p.file_name().unwrap().to_string_lossy();
        if read(&data).is_ok_and(|d| !d.title_lines.is_empty()) {
            with_title += 1;
        }
        for ops in EDITS {
            if let Err(e) = fuzzing::title_edit(&data, ops) {
                panic!("{name}, edits {ops:?}: {e}");
            }
        }
    }
    with_title
}

#[test]
fn edited_title_lines_keep_the_rest_on_our_fixtures() {
    let mut files = Vec::new();
    fdx_files(&fixtures_dir(), &mut files);
    assert!(check_all(&files) >= 2, "the fixtures hold title pages");
}

#[test]
fn edited_title_lines_keep_the_rest_on_the_corpus() {
    let mut files = Vec::new();
    fdx_files(&corpus_dir(), &mut files);
    if files.is_empty() {
        if std::env::var("COLDOPEN_REQUIRE_CORPUS").is_ok_and(|v| v == "1") {
            panic!("COLDOPEN_REQUIRE_CORPUS=1 but there is no corpus");
        }
        eprintln!("\n*** corpus/ is missing: the title edit check on the corpus is skipped ***\n");
        return;
    }
    let n = check_all(&files);
    eprintln!(
        "title edits checked on {} files, {n} with title pages",
        files.len()
    );
}

/// In our fixture the first line is `Full`, the fourth has a run in
/// italics and the fifth has tab stops. Editing the second line keeps all
/// of that, and the edited line keeps its own attributes as spelled.
#[test]
fn an_edited_line_and_its_neighbours_keep_what_the_file_held() {
    let data = std::fs::read(fixtures_dir().join("title-page.fdx")).unwrap();
    let doc = read(&data).unwrap();
    let mut lines = doc.title_lines.clone();
    lines[1].text = RichText::new(vec![Run::styled(
        "THE LAST LAMP",
        Style {
            bold: true,
            ..Style::PLAIN
        },
    )]);
    let written = coldopen_fdx::write(&Doc {
        blocks: doc.blocks(),
        format: &doc.format,
        title_lines: &lines,
        kept: Some(&doc.kept),
        declare_utf8: false,
    });
    assert!(written.losses.is_empty());
    let out = written.text;
    let original = String::from_utf8(data).unwrap();
    // Every other paragraph, as the file has it.
    for unchanged in [
        r#"<Paragraph Alignment="Full" FirstIndent="0.00" Leading="Regular" LeftIndent="1.00" RightIndent="7.50" SpaceBefore="0" Spacing="1" StartsNewPage="No">"#,
        r#"<Tabstop Position="6.00" Type="Left"/>"#,
        r#"<Text>Ada </Text><Text Style="Italic">Quill</Text>"#,
        "<Paragraph>\n        <Text>No attributes at all.</Text>",
    ] {
        assert!(original.contains(unchanged));
        assert!(out.contains(unchanged), "{unchanged}");
    }
    // The edited line keeps its other attributes and drops AllCaps, since
    // its text is now the model's.
    assert!(
        out.contains(
            r#"<Paragraph Alignment="Center" FirstIndent="0.00" Leading="Regular" LeftIndent="1.00" RightIndent="7.50" SpaceBefore="0" Spacing="1" StartsNewPage="No">"#
        ),
        "{out}"
    );
    assert!(
        out.contains(r#"<Text Style="Bold">THE LAST LAMP</Text>"#),
        "{out}"
    );
    let again = read(out.as_bytes()).unwrap();
    assert_eq!(again.title_lines, lines);
}
