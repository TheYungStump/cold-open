//! Lays out a Fountain file and writes its page view and PDF:
//! `cargo run -p coldopen-layout --example export -- in.fountain out.pdf`.
use coldopen_layout::{Fonts, Options, ValidFormat, layout, pdf};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(&args[1]).expect("read input");
    let script = coldopen_fountain::parse(&src).script;
    let fonts = Fonts::new().expect("fonts");
    let l = layout(&script, &ValidFormat::default(), &Options::default());
    print!("{}", l.page_view(&fonts));
    let out = pdf::write(
        &l,
        &fonts,
        &pdf::PdfOptions {
            watermark: args.get(3).cloned(),
            title: None,
        },
    )
    .expect("pdf");
    std::fs::write(&args[2], &out.bytes).expect("write pdf");
    eprintln!(
        "{} pages, {} bytes, warnings {:?}",
        out.pages,
        out.bytes.len(),
        out.warnings
    );
}
