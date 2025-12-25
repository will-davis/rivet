use eframe::egui;
use egui_extras::TableBuilder;
use crate::mft_indexer::Indexer;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;
use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;

#[derive(PartialEq, Clone, Copy)]
enum SortColumn {
    Name,
    Path,
    Size,
    Modified,
}

fn format_filetime(filetime: i64) -> String {
    if filetime == 0 { return "---".to_string(); }
    let unix_secs = (filetime / 10_000_000) - 11_644_473_600;
    if unix_secs < 0 { return "---".to_string(); }
    
    let dt = chrono::DateTime::from_timestamp(unix_secs, 0);
    match dt {
        Some(dt) => dt.format("%Y-%m-%d %H:%M").to_string(),
        None => "---".to_string(),
    }
}

fn format_size(bytes: u64) -> String {
    if bytes == 0 { return "0 KB".to_string(); }
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB { format!("{:.2} GB", bytes as f64 / GB as f64) }
    else if bytes >= MB { format!("{:.1} MB", bytes as f64 / MB as f64) }
    else if bytes >= KB { format!("{} KB", bytes / KB) }
    else { format!("{} B", bytes) }
}

pub struct RivetApp {
    indexer: Arc<Indexer>,
    search_query: String,
    results: Vec<u64>, 
    cancel_token: CancellationToken,
    sort_column: SortColumn,
    sort_ascending: bool,
}

impl RivetApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, indexer: Arc<Indexer>, cancel_token: CancellationToken) -> Self {
        Self {
            indexer,
            search_query: String::new(),
            results: Vec::new(),
            cancel_token,
            sort_column: SortColumn::Name,
            sort_ascending: true,
        }
    }

    fn open_file(&self, path: &str) {
        unsafe {
            ShellExecuteW(
                HWND::default(),
                &HSTRING::from("open"),
                &HSTRING::from(path),
                None,
                None,
                SW_SHOW,
            );
        }
    }

    fn open_folder(&self, path: &str) {
        unsafe {
            // /select, <path> highlights the file in Explorer
            let cmd = format!("/select,\"{}\"", path);
            ShellExecuteW(
                HWND::default(),
                &HSTRING::from("open"),
                &HSTRING::from("explorer.exe"),
                &HSTRING::from(cmd),
                None,
                SW_SHOW,
            );
        }
    }

    fn perform_search(&mut self) {
        if self.search_query.is_empty() {
            self.results.clear();
            return;
        }

        let query = self.search_query.to_lowercase();
        let mut matches = Vec::new();

        for entry in self.indexer.records.iter() {
            if entry.name.to_lowercase().contains(&query) {
                matches.push(*entry.key());
            }
            if matches.len() > 10000 { break; }
        }

        self.results = matches;
        self.sort_results();
    }

    fn sort_results(&mut self) {
        let indexer = &self.indexer;
        let ascending = self.sort_ascending;
        
        match self.sort_column {
            SortColumn::Name => {
                self.results.sort_by(|a, b| {
                    let name_a = indexer.records.get(a).map(|r| r.name.clone()).unwrap_or_default();
                    let name_b = indexer.records.get(b).map(|r| r.name.clone()).unwrap_or_default();
                    if ascending { name_a.cmp(&name_b) } else { name_b.cmp(&name_a) }
                });
            },
            SortColumn::Path => {
                self.results.sort_by(|a, b| {
                    let path_a = indexer.get_full_path(*a, 'C');
                    let path_b = indexer.get_full_path(*b, 'C');
                    if ascending { path_a.cmp(&path_b) } else { path_b.cmp(&path_a) }
                });
            },
            SortColumn::Modified => {
                self.results.sort_by(|a, b| {
                    let mod_a = indexer.records.get(a).map(|r| r.modified).unwrap_or(0);
                    let mod_b = indexer.records.get(b).map(|r| r.modified).unwrap_or(0);
                    if ascending { mod_a.cmp(&mod_b) } else { mod_b.cmp(&mod_a) }
                });
            },
            SortColumn::Size => {
                self.results.sort_by(|a, b| {
                    let size_a = indexer.records.get(a).map(|r| r.size).unwrap_or(0);
                    let size_b = indexer.records.get(b).map(|r| r.size).unwrap_or(0);
                    if ascending { size_a.cmp(&size_b) } else { size_b.cmp(&size_a) }
                });
            },
        }
    }
}

impl eframe::App for RivetApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.cancel_token.cancel();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🔍").size(20.0));
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search files...")
                        .desired_width(f32::INFINITY)
                        .lock_focus(true)
                );
                if response.changed() {
                    self.perform_search();
                }
            });
            ui.add_space(8.0);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let table = TableBuilder::new(ui)
                .striped(true)
                .resizable(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(egui_extras::Column::initial(250.0).resizable(true).at_least(100.0).clip(true)) // Name
                .column(egui_extras::Column::initial(400.0).resizable(true).at_least(100.0).clip(true)) // Path
                .column(egui_extras::Column::initial(100.0).resizable(true).at_least(50.0)) // Size
                .column(egui_extras::Column::initial(150.0).resizable(true).at_least(100.0)) // Date Modified
                .header(20.0, |mut header| {
                    header.col(|ui| {
                        let text = if self.sort_column == SortColumn::Name {
                            format!("Name {}", if self.sort_ascending { "🔼" } else { "🔽" })
                        } else { "Name".to_string() };
                        if ui.button(text).clicked() {
                            if self.sort_column == SortColumn::Name { self.sort_ascending = !self.sort_ascending; }
                            else { self.sort_column = SortColumn::Name; self.sort_ascending = true; }
                            self.sort_results();
                        }
                    });
                    header.col(|ui| {
                        let text = if self.sort_column == SortColumn::Path {
                            format!("Path {}", if self.sort_ascending { "🔼" } else { "🔽" })
                        } else { "Path".to_string() };
                        if ui.button(text).clicked() {
                            if self.sort_column == SortColumn::Path { self.sort_ascending = !self.sort_ascending; }
                            else { self.sort_column = SortColumn::Path; self.sort_ascending = true; }
                            self.sort_results();
                        }
                    });
                    header.col(|ui| {
                        let text = if self.sort_column == SortColumn::Size {
                            format!("Size {}", if self.sort_ascending { "🔼" } else { "🔽" })
                        } else { "Size".to_string() };
                        if ui.button(text).clicked() {
                            if self.sort_column == SortColumn::Size { self.sort_ascending = !self.sort_ascending; }
                            else { self.sort_column = SortColumn::Size; self.sort_ascending = true; }
                            self.sort_results();
                        }
                    });
                    header.col(|ui| {
                        let text = if self.sort_column == SortColumn::Modified {
                            format!("Date Modified {}", if self.sort_ascending { "🔼" } else { "🔽" })
                        } else { "Date Modified".to_string() };
                        if ui.button(text).clicked() {
                            if self.sort_column == SortColumn::Modified { self.sort_ascending = !self.sort_ascending; }
                            else { self.sort_column = SortColumn::Modified; self.sort_ascending = true; }
                            self.sort_results();
                        }
                    });
                });

            table.body(|body| {
                body.rows(22.0, self.results.len(), |mut row| {
                    let row_index = row.index();
                    let id = self.results[row_index];
                    let full_path = self.indexer.get_full_path(id, 'C');
                    if let Some(record) = self.indexer.records.get(&id) {
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if ui.button("🚀").on_hover_text("Open/Run File").clicked() {
                                    self.open_file(&full_path);
                                }
                                ui.label(if record.is_dir { "📁" } else { "📄" });
                                ui.add(egui::Label::new(&record.name).truncate());
                            });
                        });
                        row.col(|ui| {
                            ui.horizontal(|ui| {
                                if ui.button("📂").on_hover_text("Open in Explorer").clicked() {
                                    self.open_folder(&full_path);
                                }
                                ui.add(egui::Label::new(egui::RichText::new(&full_path).color(ui.visuals().weak_text_color())).truncate());
                            });
                        });
                        row.col(|ui| {
                            if record.is_dir {
                                ui.label("");
                            } else {
                                ui.label(format_size(record.size));
                            }
                        });
                        row.col(|ui| {
                            ui.label(format_filetime(record.modified));
                        });
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{} files indexed", self.indexer.records.len()));
                ui.separator();
                ui.label(format!("{} results", self.results.len()));
                if self.indexer.records.len() == 0 {
                    ui.separator();
                    ui.spinner();
                    ui.label("Indexing C:\\...");
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new("Rivet Alpha").text_style(egui::TextStyle::Small).weak());
                });
            });
        });

        // Request a repaint to keep UI updated if background indexing is happening
        if self.results.is_empty() && !self.search_query.is_empty() {
             ctx.request_repaint();
        }
    }
}
