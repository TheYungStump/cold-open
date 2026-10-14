//! Lays out an .fdx file with its own page format and prints the page view:
//! `cargo run -p coldopen-layout --example fdx_pages -- script.fdx`.
//! A development aid for comparing against reference PDFs.
use coldopen_layout::{Engine, Fonts, Options, ValidFormat};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bytes = std::fs::read(&args[1]).expect("read input");
    let doc = coldopen_fdx::read(&bytes).expect("fdx");
    let format = ValidFormat::new(doc.format.clone()).expect("valid format");
    let options = Options {
        overrides: doc.overrides.clone(),
        title_lines: Some(doc.title_lines.clone()),
        ..Options::default()
    };
    let layout = Engine::new(&doc.script, &format, &options).run();
    let fonts = Fonts::new().expect("fonts");
    print!("{}", layout.page_view(&fonts));
    eprintln!(
        "{} body pages (+{} title)",
        layout.pages.len(),
        layout.title.len()
    );
}
