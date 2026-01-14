use eframe::egui;
use crate::api;
use crate::parsers;

pub fn run() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 700.0]),
        ..Default::default()
    };
    eframe::run_native(
        "BIOS Dump Tool - ACPI & SMBIOS Viewer",
        options,
        Box::new(|cc| Ok(Box::new(DumpApp::new(cc)))),
    )
}

#[derive(PartialEq)]
enum Tab {
    Hex,
    Parsed,
}

enum Selection {
    None,
    Acpi(String), // signature
    Smbios(usize, u8, u8, u16), // offset, type_id, length, handle
}

struct DumpApp {
    acpi_tables: Vec<String>,
    smbios_data: Vec<u8>,
    smbios_list: Vec<(usize, u8, u8, u16, String)>, // offset, type, length, handle, label
    
    selected_item: Selection,
    active_tab: Tab,
    
    // Cache for right panel content
    cached_hex: String,
    cached_parsed: String,
}

impl DumpApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Load data on startup
        let acpi_tables = api::enum_system_firmware_tables(api::SIG_ACPI).unwrap_or_default();
        let smbios_data = api::get_smbios_data().unwrap_or_default();
        
        let mut smbios_list = Vec::new();
        if !smbios_data.is_empty() {
             let (start_offset, _) = parsers::parse_raw_smbios_data_header(&smbios_data)
                 .map(|(_, off)| (off, 0)) // We don't need header details here
                 .unwrap_or((0, 0));
                 
             let mut current_off = start_offset;
             while current_off < smbios_data.len() {
                 if let Ok((header, next_off)) = parsers::parse_smbios_structure(&smbios_data, current_off) {
                     let mut label = format!("Type {} (Handle 0x{:04X})", header.type_id, header.handle);
                     let type_name = match header.type_id {
                        0 => "BIOS Info", 1 => "System Info", 2 => "Baseboard",
                        3 => "Chassis", 4 => "Processor", 17 => "Memory", _ => ""
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

        Self {
            acpi_tables,
            smbios_data,
            smbios_list,
            selected_item: Selection::None,
            active_tab: Tab::Hex,
            cached_hex: String::new(),
            cached_parsed: String::new(),
        }
    }

    fn select_acpi(&mut self, signature: String) {
        self.selected_item = Selection::Acpi(signature.clone());
        // Fetch data
        if let Ok(data) = api::get_system_firmware_table(api::SIG_ACPI, &signature) {
             self.update_cache(&data, "ACPI", &signature);
        } else {
             self.cached_hex = "Error fetching table".to_string();
             self.cached_parsed = "Error fetching table".to_string();
        }
    }

    fn select_smbios(&mut self, offset: usize, type_id: u8, length: u8, handle: u16) {
        self.selected_item = Selection::Smbios(offset, type_id, length, handle);
        // Extract chunk
        let end = offset + length as usize; // This is just the formatted part? No, structure goes until double null.
        // Wait, for hex dump usually we want the whole structure including strings.
        // `parsers::parse_smbios_structure` returns next_offset which is the end of the whole structure.
        // I need to re-parse to find the end or store it.
        // For simplicity, let's re-parse to find end.
        if let Ok((_, next_off)) = parsers::parse_smbios_structure(&self.smbios_data, offset) {
             // Clone data to release borrow on self.smbios_data before calling methods that borrow self mutably
             let data_vec = self.smbios_data[offset..next_off].to_vec();
             self.update_cache(&data_vec, "SMBIOS", &format!("Type {}", type_id));
        }
    }

    fn update_cache(&mut self, data: &[u8], cat: &str, id: &str) {
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
             } else {
                  out.push_str("Error parsing ACPI Header\n");
             }
        } else if cat == "SMBIOS" {
             // We need to parse details.
             // data is the full structure blob.
             // parse_smbios_details expects the full blob or at least starting at offset?
             // My parsers take `data` (full buffer usually) and `offset`. 
             // If I pass the slice `data` (which is just the structure), offset should be 0.
             
             // Extract header again from slice
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
}

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
        // Align
        out.push_str(&format!("{:04X}  {:<48}  {}\n", offset, hex_str, ascii_part));
    }
    out
}

impl eframe::App for DumpApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
             ui.horizontal(|ui| {
                 // Left Panel - Tree
                 ui.push_id("left_panel", |ui| {
                     egui::ScrollArea::vertical().min_scrolled_height(600.0).show(ui, |ui| {
                         ui.set_width(300.0);
                         ui.heading("Firmware Tables");
                         ui.separator();
                         
                         egui::CollapsingHeader::new("ACPI Tables")
                             .default_open(true)
                             .show(ui, |ui| {
                                 let mut clicked_acpi = None;
                                 for t in &self.acpi_tables {
                                     if ui.selectable_label(match &self.selected_item {
                                         Selection::Acpi(s) => s == t,
                                         _ => false
                                     }, t).clicked() {
                                         clicked_acpi = Some(t.clone());
                                     }
                                 }
                                 if let Some(t) = clicked_acpi {
                                     self.select_acpi(t);
                                 }
                             });

                         egui::CollapsingHeader::new("SMBIOS Data")
                             .default_open(true)
                             .show(ui, |ui| {
                                 let mut clicked_smbios = None;
                                 for (offset, type_id, length, handle, label) in &self.smbios_list {
                                      let is_selected = match &self.selected_item {
                                          Selection::Smbios(off, _, _, _) => *off == *offset,
                                          _ => false
                                      };
                                      if ui.selectable_label(is_selected, label).clicked() {
                                          clicked_smbios = Some((*offset, *type_id, *length, *handle));
                                      }
                                 }
                                 if let Some((off, tid, len, hdl)) = clicked_smbios {
                                      self.select_smbios(off, tid, len, hdl);
                                 }
                             });
                     });
                 });

                 ui.separator();

                 // Right Panel - Tabs
                 ui.push_id("right_panel", |ui| {
                      ui.vertical(|ui| {
                          ui.horizontal(|ui| {
                              if ui.selectable_label(self.active_tab == Tab::Hex, "Hex View").clicked() {
                                  self.active_tab = Tab::Hex;
                              }
                              if ui.selectable_label(self.active_tab == Tab::Parsed, "Parsed View").clicked() {
                                  self.active_tab = Tab::Parsed;
                              }
                          });
                          ui.separator();
                          
                          egui::ScrollArea::vertical().show(ui, |ui| {
                              ui.add_sized(ui.available_size(), egui::TextEdit::multiline(
                                  match self.active_tab {
                                      Tab::Hex => &mut self.cached_hex,
                                      Tab::Parsed => &mut self.cached_parsed,
                                  }
                              ).font(egui::TextStyle::Monospace));
                          });
                      });
                 });
             });
        });
    }
}
