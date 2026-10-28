//! RExcel: a desktop spreadsheet application in the style of Microsoft Excel.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod charts;
mod clipboard;
mod demo;
mod dialogs;
mod editor;
mod fileio;
mod fonts;
mod format_dialog;
mod grid;
mod numfmt_util;
mod png;
mod print;
mod print_chart;
mod print_os;
mod ribbon;
mod sheet_tabs;
mod spell;
mod status_bar;
mod theme;

use std::path::PathBuf;

/// Command-line options.
#[derive(Default, Clone)]
pub struct Args {
    pub file: Option<PathBuf>,
    pub demo: bool,
    pub screenshot: Option<PathBuf>,
    /// Scripted UI actions run before taking a screenshot (testing aid).
    pub actions: Vec<String>,
}

fn parse_args() -> Args {
    let mut a = Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.as_str() {
            "--demo" => a.demo = true,
            "--screenshot" => a.screenshot = it.next().map(PathBuf::from),
            "--action" => {
                if let Some(x) = it.next() {
                    a.actions.push(x);
                }
            }
            "--help" | "-h" => {
                println!("Usage: rexcel [FILE] [--demo] [--screenshot OUT.png] [--action NAME]...");
                std::process::exit(0);
            }
            _ if !s.starts_with("--") && a.file.is_none() => a.file = Some(PathBuf::from(s)),
            _ => eprintln!("rexcel: ignoring unknown argument {}", s),
        }
    }
    a
}

fn main() -> eframe::Result {
    let args = parse_args();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Book1 - RExcel")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([640.0, 400.0])
            .with_drag_and_drop(true)
            .with_icon(std::sync::Arc::new(egui::IconData {
                rgba: include_bytes!("../assets/icon/rexcel-128.rgba").to_vec(),
                width: 128,
                height: 128,
            })),
        ..Default::default()
    };
    eframe::run_native(
        "RExcel",
        options,
        Box::new(move |cc| {
            fonts::install(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::App::new(&cc.egui_ctx, args)))
        }),
    )
}
