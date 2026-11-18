#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod curve;
mod cutout;
mod gen;
mod math;
mod model;
mod panels;
mod props;
mod render;
mod spec;
mod spm;

use eframe::egui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if let Some(p) = args.iter().position(|a| a == "--write-settings-list").and_then(|i| args.get(i + 1)) {
        let text = format!("{}{}", include_str!("../SETTINGS_HEADER.txt"), props::inert_list());
        std::fs::write(p, text).expect("write the settings list");
        return Ok(());
    }
    let start = args.iter().position(|a| a == "--template").and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| "Broadleaf".into());
    let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("TreeCraft Modeler 0.3.0")
        .with_inner_size([2560.0, 1440.0])
        .with_min_inner_size([1000.0, 640.0]);
    if shot.is_some() {
        // Check runs: don't take focus or cover the person's windows; fixed 2K size for the PNG.
        viewport = viewport.with_active(false);
        #[cfg(windows)]
        {
            viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnBottom);
        }
    } else {
        viewport = viewport.with_maximized(true);
    }
    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    #[cfg(target_os = "macos")]
    if shot.is_some() {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        options.event_loop_builder = Some(Box::new(|b| {
            b.with_activate_ignoring_other_apps(false);
        }));
    }
    eframe::run_native("TreeCraft", options, Box::new(move |cc| {
        let mut app = app::TreeCraft::new(cc, shot, &start);
        app.apply_args(&args);
        Ok(Box::new(app))
    }))
}
