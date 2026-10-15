//! `coldopen-app [script]`: the Cold Open desktop app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use coldopen_app::{App, Boot};

fn main() -> iced::Result {
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let boot = Boot {
        file,
        ..Boot::default()
    };
    iced::application(move || App::new(boot.clone()), App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .settings(coldopen_app::app::iced_settings())
        .window_size(iced::Size::new(1280.0, 860.0))
        .run()
}
