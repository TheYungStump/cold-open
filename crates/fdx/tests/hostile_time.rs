//! Reading and writing .fdx take time in proportion to the input, even for
//! hostile shapes (docs/specs/fdx.md §10 rules 7 and 8). Each check times
//! one size and four times that size; linear work grows about 4×,
//! quadratic about 16×. Timing the growth, not a fixed budget, keeps the
//! check fair on slow machines and in debug builds.

use std::time::{Duration, Instant};

use coldopen_fdx::{Blk, Doc, read, write};
use coldopen_screenplay::RichText;

/// The fastest of three runs, which damps scheduler noise.
fn time(f: impl Fn()) -> Duration {
    (0..3)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// Reads the file, writes it back, then writes it with every block's text
/// changed, so every kept paragraph is generated and carries what it can.
fn work(text: &str) {
    let Ok(doc) = read(text.as_bytes()) else {
        return;
    };
    std::hint::black_box(doc.write_back());
    let edited: Vec<_> = doc
        .script
        .elements
        .iter()
        .map(|e| {
            let mut e = e.clone();
            if !e.text.is_empty() {
                e.text = RichText::plain(format!("x{}", e.text.plain_text()));
            }
            e
        })
        .collect();
    // Every other block, so anchors and kept paragraphs are looked up for
    // blocks that are gone.
    let blocks: Vec<Blk<'_>> = edited
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(i, element)| Blk {
            id: i as u64 + 1,
            element,
            spacing: None,
        })
        .collect();
    std::hint::black_box(write(&Doc {
        blocks,
        format: &doc.format,
        title_lines: &doc.title_lines,
        kept: Some(&doc.kept),
        declare_utf8: false,
    }));
}

/// Fails if the work for `shape(4 * n)` grows like n², not n.
fn grows_linearly(name: &str, n: usize, shape: impl Fn(usize) -> String) {
    let small = shape(n);
    let large = shape(4 * n);
    let a = time(|| work(&small));
    let b = time(|| work(&large));
    let ratio = b.as_secs_f64() / a.as_secs_f64().max(1e-6);
    println!(
        "{name}: {} bytes {a:?}, {} bytes {b:?}, ratio {ratio:.1}",
        small.len(),
        large.len()
    );
    assert!(
        ratio < 9.0,
        "{name} grows like the square of its size: ratio {ratio:.1}"
    );
}

fn file(content: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<FinalDraft>\n  <Content>{content}\n  </Content>\n</FinalDraft>\n"
    )
}

#[test]
fn many_kept_paragraphs() {
    grows_linearly("kept paragraphs", 1_000, |n| {
        file(&"\n    <Paragraph Number=\"1\" Type=\"Scene Heading\">\n      <SceneProperties Length=\"1/8\" Page=\"1\"/>\n      <Text Font=\"Courier\" RevisionID=\"3\">INT. A - DAY</Text>\n    </Paragraph>".repeat(n))
    });
}

#[test]
fn many_loose_children() {
    grows_linearly("loose children", 1_000, |n| {
        file(&"\n    <!-- c -->\n    <Unknown A=\"1\"/>\n    <Paragraph Type=\"Action\">\n      <Text>Rain.</Text>\n    </Paragraph>".repeat(n))
    });
}

#[test]
fn one_paragraph_of_many_distinct_runs() {
    grows_linearly("distinct runs", 1_000, |n| {
        let runs: String = (0..n)
            .map(|i| format!("<Text RevisionID=\"{i}\">w{i} </Text>"))
            .collect();
        file(&format!(
            "\n    <Paragraph Type=\"Action\">{runs}</Paragraph>"
        ))
    });
}

#[test]
fn one_long_text_of_references() {
    grows_linearly("references", 5_000, |n| {
        file(&format!(
            "\n    <Paragraph Type=\"Action\">\n      <Text>{}</Text>\n    </Paragraph>",
            "&amp;&#x27;&nope;&#1; ".repeat(n)
        ))
    });
}

#[test]
fn many_repairs() {
    grows_linearly("repairs", 200, |n| {
        file(&"\n    <Paragraph Type=\"Action\">\n      <Text>a</Bad>\n    </Paragraph>\n    <Paragraph <<\n".repeat(n))
    });
}

#[test]
fn deep_nesting_again_and_again() {
    grows_linearly("nesting", 50, |n| {
        let deep = format!("{}x{}", "<a>".repeat(80), "</a>".repeat(80));
        file(
            &format!(
                "
    <Paragraph Type=\"Action\">{deep}<Text>t</Text></Paragraph>"
            )
            .repeat(n),
        )
    });
}

/// CL3 (the reviews of phases 4–5b, 2026-10-28): elements nested past the
/// depth limit, each with a character the reader must repair. Every repair
/// marked every element still open, so the time grew as nesting times
/// repairs: 320 KB took 7.6 s.
#[test]
fn deep_nesting_with_repairs() {
    grows_linearly("nesting with repairs", 5_000, |n| {
        format!(
            "<?xml version=\"1.0\"?><FinalDraft DocumentType=\"Script\" Version=\"5\"><Content>{}",
            "<a>\u{1}".repeat(n)
        )
    });
}
