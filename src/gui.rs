use eframe::egui;
use crate::api;
use crate::parsers;
use std::io::Write;

/// Entry point for launching the GUI version of the BIOS Dump Tool.
///
/// Sets up window options, loads the application icon, and starts the `eframe` event loop.
///
/// # Returns
/// `Result<(), eframe::Error>`
pub fn run() -> Result<(), eframe::Error> {
    // Load icon for the taskbar/window
    let icon_data = include_bytes!("../assets/icon.ico");
    
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };

    // Try to load and set the icon
    if let Ok(image) = image::load_from_memory(icon_data) {
        let image = image.to_rgba8();
        let (width, height) = image.dimensions();
        options.viewport.icon = Some(std::sync::Arc::new(egui::IconData {
            rgba: image.into_raw(),
            width,
            height,
        }));
    }

    eframe::run_native(
        "BIOS Dump Tool - ACPI & SMBIOS Viewer",
        options,
        Box::new(|cc| Ok(Box::new(DumpApp::new(cc)))),
    )
}

/// Represents the two viewing modes for table data.
#[derive(PartialEq)]
enum Tab {
    /// Raw hexadecimal representation.
    Hex,
    /// Human-readable interpreted representation.
    Parsed,
}

/// Tracks the currently selected item in the sidebar.
enum Selection {
    /// Nothing is selected.
    None,
    /// An ACPI table is selected.
    Acpi(api::AcpiTableInfo),
    /// An SMBIOS structure is selected (offset, type_id).
    Smbios(usize, u8),
}

#[allow(dead_code)]
impl Selection {
    /// Returns true if no item is selected.
    fn is_none(&self) -> bool {
        matches!(self, Selection::None)
    }
}

/// The main application state for the egui interface.
///
/// Manages discovered tables, UI state (selections, tabs, filters), and cached data views.
struct DumpApp {
    /// List of discovered ACPI tables.
    acpi_tables: Option<Vec<api::AcpiTableInfo>>,
    /// Raw SMBIOS data buffer.
    smbios_data: Option<Vec<u8>>,
    /// List of parsed SMBIOS structures for the sidebar.
    smbios_list: Vec<(usize, u8, u8, u16, String)>, // offset, type, length, handle, label
    
    /// The currently selected table or structure.
    selected_item: Selection,
    /// The active view tab (Hex or Parsed).
    active_tab: Tab,
    
    /// Cached hex dump string of the selected item.
    cached_hex: String,
    /// Cached parsed/interpreted string of the selected item.
    cached_parsed: String,

    /// Text used to filter the sidebar table list.
    sidebar_filter: String,
    /// Text for searching within the current data view.
    search_query: String,
    /// Whether the search panel (Ctrl+F) is currently visible.
    search_panel_open: bool,
}

impl DumpApp {
    /// Creates a new instance of the application with default state.
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            acpi_tables: None,
            smbios_data: None,
            smbios_list: Vec::new(),
            selected_item: Selection::None,
            active_tab: Tab::Hex,
            cached_hex: String::new(),
            cached_parsed: String::new(),
            sidebar_filter: String::new(),
            search_query: String::new(),
            search_panel_open: false,
        }
    }

    /// Triggers the combined discovery of ACPI tables and updates the state.
    fn load_acpi(&mut self) {
        self.acpi_tables = Some(api::load_acpi_tables_combined());
    }

    /// Triggers the retrieval and parsing of SMBIOS data and updates the state.
    fn load_smbios(&mut self) {
        let smbios_data = api::get_smbios_data().unwrap_or_default();
        
        let mut smbios_list = Vec::new();
        if !smbios_data.is_empty() {
             let (start_offset, _) = parsers::parse_raw_smbios_data_header(&smbios_data)
                 .map(|(_, off)| (off, 0))
                 .unwrap_or((0, 0));
                 
             let mut current_off = start_offset;
             while current_off < smbios_data.len() {
                 if let Ok((header, next_off)) = parsers::parse_smbios_structure(&smbios_data, current_off) {
                     let mut label = format!("Type {} (Handle 0x{:04X})", header.type_id, header.handle);
                     let type_name = match header.type_id {
                        0 => "BIOS Info", 1 => "System Info", 2 => "Baseboard",
                        3 => "Chassis", 4 => "Processor", 7 => "Cache Info",
                        9 => "System Slots", 11 => "OEM Strings", 17 => "Memory",
                        32 => "Boot Info", _ => ""
                     };
                     if !type_name.is_empty() {
                          label.push_str(" - ");
                          label.push_str(type_name);
                     }
                     
                     smbios_list.push((current_off, header.type_id, header.length, header.handle, label));
                     
                     if next_off == current_off { break; }
                     current_off = next_off;
                 } else {
                     break;
                 }
             }
        }
        self.smbios_data = Some(smbios_data);
        self.smbios_list = smbios_list;
    }

    /// Handles the selection of an ACPI table and updates the detail views.
    fn select_acpi(&mut self, info: api::AcpiTableInfo) {
        self.selected_item = Selection::Acpi(info.clone());
        
        let result = if let Some(ref path) = info.registry_path {
            api::get_acpi_table_by_path(path)
        } else {
            api::get_system_firmware_table(api::SIG_ACPI, &info.signature)
        };

        match result {
            Ok(data) => self.update_cache(&data, "ACPI", &info.signature),
            Err(e) => {
                self.cached_hex = format!("Error: {}", e);
                self.cached_parsed = format!("Error: {}", e);
            }
        }
    }

    /// Handles the selection of an SMBIOS structure and updates the detail views.
    fn select_smbios(&mut self, offset: usize, type_id: u8) {
        self.selected_item = Selection::Smbios(offset, type_id);
        if let Some(ref data) = self.smbios_data {
            if let Ok((_, next_off)) = parsers::parse_smbios_structure(data, offset) {
                 let data_vec = data[offset..next_off].to_vec();
                 self.update_cache(&data_vec, "SMBIOS", &format!("Type {}", type_id));
            }
        }
    }

    /// Updates the internal hex and parsed text caches for the selected data block.
    fn update_cache(&mut self, data: &[u8], cat: &str, _id: &str) {
        // Hex Dump
        self.cached_hex = hex_dump_str(data);
        
        // Parsed
        let mut out = String::new();
        if cat == "ACPI" {
             if let Ok(header) = parsers::parse_acpi_header(data) {
                  out.push_str(&format!("Signature: {}\n", header.signature));
                  out.push_str(&format!("Length:    {}\n", header.length));
                  out.push_str(&format!("OEM ID:    {}\n", header.oem_id));
                  out.push_str(&format!("Table ID:  {}\n", header.oem_table_id));
                  out.push_str(&format!("Revision:  {}\n", header._revision));
                  
                  if header.signature == "XSDT" {
                       out.push_str("\n====================\nXSDT Entries:\n");
                       let mut addr_map = std::collections::HashMap::new();
                       if let Some(ref all_tables) = self.acpi_tables {
                            if let Some(fadt_info) = all_tables.iter().find(|t| t.signature == "FACP" || t.signature == "FADT") {
                                 let data = if let Some(ref path) = fadt_info.registry_path {
                                      api::get_acpi_table_by_path(path).ok()
                                 } else {
                                      api::get_system_firmware_table(api::SIG_ACPI, &fadt_info.signature).ok()
                                 };
                                 
                                 if let Some(d) = data {
                                      let refs = parsers::parse_fadt_references(&d);
                                      for (a, s) in refs { addr_map.insert(a, s); }
                                 }
                            }
                       }

                       let empty_lookup = std::collections::HashMap::new();
                       if let Some(entries) = parsers::parse_xsdt_entries(data, &empty_lookup) {
                            for (i, addr, _) in entries {
                                 let label = addr_map.get(&addr).cloned();
                                 if let Some(sig) = label {
                                      out.push_str(&format!("Entry{:<12}0x{:016X} ({})\n", i, addr, sig));
                                 } else {
                                      out.push_str(&format!("Entry{:<12}0x{:016X}\n", i, addr));
                                 }
                            }
                       }
                  }
             } else {
                  out.push_str("Error parsing ACPI Header\n");
             }
        } else if cat == "SMBIOS" {
             if let Ok((header, _)) = parsers::parse_smbios_structure(data, 0) {
                  let strings = parsers::get_smbios_strings(data, 0, header.length);
                  
                  out.push_str(&format!("Type {} (Handle 0x{:04X})\n", header.type_id, header.handle));
                  out.push_str(&format!("Length: {}\n", header.length));
                  out.push_str("====================\n");
                  
                  if let Some(details) = parsers::parse_smbios_details(header.type_id, data, 0, header.length, &strings) {
                      for (k, v) in details {
                           out.push_str(&format!("{:25}: {}\n", k, v));
                      }
                  } else {
                      if !strings.is_empty() {
                          out.push_str("Strings:\n");
                          for (i, s) in strings.iter().enumerate() {
                              out.push_str(&format!("  {}: {}\n", i+1, s));
                          }
                      } else {
                          out.push_str("No strings.\n");
                      }
                  }
             }
        }
        self.cached_parsed = out;
    }

    /// Opens a save file dialog to export the currently selected item as a raw binary file.
    fn export_raw(&self) {
        let (data, default_name) = match &self.selected_item {
            Selection::Acpi(info) => {
                let result = if let Some(ref path) = info.registry_path {
                    api::get_acpi_table_by_path(path)
                } else {
                    api::get_system_firmware_table(api::SIG_ACPI, &info.signature)
                };

                if let Ok(data) = result {
                    (data, format!("{}_{}.aml", info.signature, info.table_id.trim()))
                } else { return; }
            }
            Selection::Smbios(off, tid) => {
                if let Some(ref smbios_data) = self.smbios_data {
                    if let Ok((_, next_off)) = parsers::parse_smbios_structure(smbios_data, *off) {
                        (smbios_data[*off..next_off].to_vec(), format!("smbios_type_{}.bin", tid))
                    } else { return; }
                } else { return; }
            }
            Selection::None => return,
        };

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .save_file() {
            if let Ok(mut file) = std::fs::File::create(path) {
                let _ = file.write_all(&data);
            }
        }
    }

    /// Opens a save file dialog to export the currently selected item's parsed view as a text file.
    fn export_parsed(&self) {
        let default_name = match &self.selected_item {
            Selection::Acpi(info) => format!("{}_{}_parsed.txt", info.signature, info.table_id.trim()),
            Selection::Smbios(_, tid) => format!("smbios_type_{}_parsed.txt", tid),
            Selection::None => return,
        };

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("Text Files", &["txt"])
            .save_file() {
            if let Ok(mut file) = std::fs::File::create(path) {
                let _ = file.write_all(self.cached_parsed.as_bytes());
            }
        }
    }

    /// Opens a folder picker to export all discovered ACPI tables as individual binary files.
    fn export_all_acpi(&self) {
        if let Some(tables) = &self.acpi_tables {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Select Folder to Export All ACPI Tables")
                .pick_folder() {
                for info in tables {
                    let result = if let Some(ref path) = info.registry_path {
                        api::get_acpi_table_by_path(path)
                    } else {
                        api::get_system_firmware_table(api::SIG_ACPI, &info.signature)
                    };

                    if let Ok(data) = result {
                        let path = folder.join(format!("{}_{}.aml", info.signature, info.table_id.trim()));
                        if let Ok(mut file) = std::fs::File::create(path) {
                            let _ = file.write_all(&data);
                        }
                    }
                }
            }
        }
    }

    /// Opens a save file dialog to export the entire raw SMBIOS information blob.
    fn export_full_smbios(&self) {
        if let Some(ref data) = self.smbios_data {
             if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Full SMBIOS Data")
                .set_file_name("smbios_raw.bin")
                .save_file() {
                if let Ok(mut file) = std::fs::File::create(path) {
                    let _ = file.write_all(data);
                }
            }
        }
    }
}

/// Generates a standardized hex dump string from a byte slice.
///
/// Each line includes the offset, 16 hex bytes, and the corresponding ASCII representation.
fn hex_dump_str(data: &[u8]) -> String {
    let mut out = String::new();
    let length = 16;
    for (i, chunk) in data.chunks(length).enumerate() {
        let offset = i * length;
        let hex_part: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
        let hex_str = hex_part.join(" ");
        let ascii_part: String = chunk.iter().map(|&b| {
            if b >= 32 && b < 127 { b as char } else { '.' }
        }).collect();
        out.push_str(&format!("{:04X}  {:<48}  {}\n", offset, hex_str, ascii_part));
    }
    out
}

impl eframe::App for DumpApp {
    /// Main UI loop for the application.
    ///
    /// Defines the sidebar (table list), central panel (data view), search panel, and top toolbar.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(320.0)
            .width_range(200.0..=500.0)
            .show(ctx, |ui| {
                ui.heading("Firmware Tables");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("🔍");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sidebar_filter)
                            .hint_text("Filter tables...")
                            .desired_width(ui.available_width()),
                    );
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let filter = self.sidebar_filter.to_lowercase();

                    egui::CollapsingHeader::new("ACPI Tables")
                        .default_open(true)
                        .show(ui, |ui| {
                            if let Some(tables) = &self.acpi_tables {
                                ui.horizontal(|ui| {
                                    if ui.button("💾 Export All to AML").clicked() {
                                        self.export_all_acpi();
                                    }
                                });
                                ui.separator();

                                let mut clicked_acpi = None;
                                for t in tables {
                                    let label = format!("{} ({})", t.signature, t.table_id.trim());
                                    if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                                        continue;
                                    }
                                    let is_selected = match &self.selected_item {
                                        Selection::Acpi(s) => s == t,
                                        _ => false,
                                    };
                                    if ui.selectable_label(is_selected, &label).clicked() {
                                        clicked_acpi = Some(t.clone());
                                    }
                                }
                                if let Some(t) = clicked_acpi {
                                    self.select_acpi(t);
                                }
                            } else if ui.button("Load ACPI Tables").clicked() {
                                self.load_acpi();
                            }
                        });

                    egui::CollapsingHeader::new("SMBIOS Data")
                        .default_open(true)
                        .show(ui, |ui| {
                            if self.smbios_data.is_some() {
                                ui.horizontal(|ui| {
                                    if ui.button("💾 Export Full Blob").clicked() {
                                        self.export_full_smbios();
                                    }
                                });
                                ui.separator();

                                let mut clicked_smbios = None;
                                    for (offset, type_id, _length, _handle, label) in &self.smbios_list {
                                        if !filter.is_empty() && !label.to_lowercase().contains(&filter) {
                                            continue;
                                        }
                                        let is_selected = match &self.selected_item {
                                            Selection::Smbios(off, _) => *off == *offset,
                                            _ => false,
                                        };
                                        if ui.selectable_label(is_selected, label).clicked() {
                                            clicked_smbios = Some((*offset, *type_id));
                                        }
                                    }
                                    if let Some((off, tid)) = clicked_smbios {
                                        self.select_smbios(off, tid);
                                    }
                            } else if ui.button("Load SMBIOS Data").clicked() {
                                self.load_smbios();
                            }
                        });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // Monitor for Ctrl+F keyboard shortcut
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::F)) {
                self.search_panel_open = !self.search_panel_open;
            }

            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.active_tab == Tab::Hex, "Hex View")
                        .clicked()
                    {
                        self.active_tab = Tab::Hex;
                    }
                    if ui
                        .selectable_label(self.active_tab == Tab::Parsed, "Parsed View")
                        .clicked()
                    {
                        self.active_tab = Tab::Parsed;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let has_selection = !matches!(self.selected_item, Selection::None);

                        if ui
                            .add_enabled(has_selection, egui::Button::new("📥 Export Parsed"))
                            .on_disabled_hover_text("Select an item to export")
                            .clicked()
                        {
                            self.export_parsed();
                        }
                        if ui
                            .add_enabled(has_selection, egui::Button::new("📦 Export Raw Binary"))
                            .on_disabled_hover_text("Select an item to export")
                            .clicked()
                        {
                            self.export_raw();
                        }
                        
                        if ui.toggle_value(&mut self.search_panel_open, "🔍 Search (Ctrl+F)").clicked() {
                        }
                    });
                });
                ui.separator();

                // Search Bar
                if self.search_panel_open {
                    ui.horizontal(|ui| {
                        ui.label("Find:");
                        let response = ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Enter text..."));
                        if self.search_panel_open {
                             response.request_focus();
                        }
                        
                        let text_to_search = match self.active_tab {
                            Tab::Hex => &self.cached_hex,
                            Tab::Parsed => &self.cached_parsed,
                        };
                        
                        if !self.search_query.is_empty() {
                            let matches = text_to_search.to_lowercase().matches(&self.search_query.to_lowercase()).count();
                            ui.label(format!("{} matches", matches));
                        }
                        
                        if ui.button("Close").clicked() {
                            self.search_panel_open = false;
                        }
                    });
                    ui.separator();
                }

                // Data Display Area
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let text = match self.active_tab {
                        Tab::Hex => &mut self.cached_hex,
                        Tab::Parsed => &mut self.cached_parsed,
                    };

                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(text)
                        .font(egui::TextStyle::Monospace)
                        .lock_focus(true),
                    );
                });
            });
        });
    }
}
