#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod mft_indexer;
mod usn_monitor;
mod gui;
mod mft_enumerator;

use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use crate::mft_indexer::Indexer;
use crate::usn_monitor::Monitor;
use crate::gui::RivetApp;

#[tokio::main]
async fn main() -> eframe::Result {
    // Load icon
    let icon = image::open("rivetfavicon.ico")
        .map(|img| {
            let img = img.to_rgba8();
            let (width, height) = img.dimensions();
            std::sync::Arc::new(eframe::egui::IconData {
                rgba: img.into_raw(),
                width,
                height,
            })
        })
        .ok();
    let cancel_token = CancellationToken::new();
    let indexer = Arc::new(Indexer::new());
    
    let monitor_indexer = Arc::clone(&indexer);
    let monitor_token = cancel_token.clone();

    // Initial indexing in background
    let bg_indexer = Arc::clone(&indexer);
    let bg_token = cancel_token.clone();
    std::thread::spawn(move || {
        println!("Starting MFT index...");
        if let Err(e) = bg_indexer.index_volume('C', &bg_token) {
            eprintln!("Failed to index MFT: {}", e);
        } else {
            println!("MFT index complete. Fetching sizes...");
            bg_indexer.fetch_sizes('C', &bg_token);
            println!("Size fetch complete.");
        }
    });

    // Start USN monitoring in background
    std::thread::spawn(move || {
        let monitor = Monitor::new(monitor_indexer);
        if let Err(e) = monitor.start_monitoring('C', &monitor_token) {
            eprintln!("Failed to start USN monitor: {}", e);
        }
    });

    let mut native_options = eframe::NativeOptions::default();
    native_options.viewport.icon = icon;
    let app_token = cancel_token.clone();
    eframe::run_native(
        "Rivet",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(RivetApp::new(cc, indexer, app_token)))
        }),
    )
}
