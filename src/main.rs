mod launcher;
mod library;
mod steam;
mod ui;

use gtk4::prelude::*;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

static TOKIO_RT: OnceLock<Runtime> = OnceLock::new();

pub fn runtime() -> &'static Runtime {
    TOKIO_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build Tokio runtime")
    })
}

fn main() {
    let _ = runtime();

    glib::set_prgname(Some("proton-trainer"));
    glib::set_application_name("Proton Trainer");

    let app = libadwaita::Application::builder()
        .application_id("io.github.labj1987.ProtonTrainer")
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_activate(|app| {
        if let Some(window) = app.windows().first() {
            window.present();
            return;
        }
        ui::build_ui(app);
    });

    std::process::exit(app.run().value());
}
