use crate::api;
use crate::parsers;
use eframe::egui;
use eframe::egui::{Color32, FontId, Galley, TextFormat};
use std::io::Write;
use std::sync::Arc;
use windows::Win32::UI::Shell::IsUserAnAdmin;

const STATUS_OK: &str = "Ready";

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
    /// Whether dark mode is enabled.
    dark_mode: bool,

    /// Status message for load/operations.
    status: String,
    /// Whether process has admin rights (affects firmware APIs).
    is_admin: bool,
    /// Parsed SMBIOS header for version info.
    smbios_header: Option<parsers::RawSMBIOSData>,

    /// Cached match positions for search.
    search_matches: Vec<usize>,
    search_current: usize,
}

impl DumpApp {
    /// Creates a new instance of the application with default state.
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Default to dark mode
        let dark_mode = cc.egui_ctx.style().visuals.dark_mode;
        let is_admin = unsafe { IsUserAnAdmin().as_bool() };

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
            dark_mode,
            status: STATUS_OK.to_string(),
            is_admin,
            smbios_header: None,
            search_matches: Vec::new(),
            search_current: 0,
        }
    }

    /// Copies the given text to the system clipboard.
    fn copy_to_clipboard(&self, ctx: &egui::Context, text: &str) {
        ctx.copy_text(text.to_string());
    }

    /// Triggers the combined discovery of ACPI tables and updates the state.
    fn load_acpi(&mut self) {
        let tables = api::load_acpi_tables_combined();
        if tables.is_empty() {
            self.status = "No ACPI tables found (admin required?)".to_string();
        } else {
            self.status = format!("Loaded {} ACPI tables", tables.len());
        }
        self.acpi_tables = Some(tables);
    }

    /// Triggers the retrieval and parsing of SMBIOS data and updates the state.
    fn load_smbios(&mut self) {
        let smbios_data = match api::get_smbios_data() {
            Ok(data) => {
                self.status = "Loaded SMBIOS data".to_string();
                data
            }
            Err(e) => {
                self.status = format!("SMBIOS load failed: {}", e);
                Vec::new()
            }
        };

        let mut smbios_list = Vec::new();
        self.smbios_header = None;
        if !smbios_data.is_empty() {
            if let Some((hdr, off)) = parsers::parse_raw_smbios_data_header(&smbios_data) {
                self.smbios_header = Some(hdr);
                let mut current_off = off;

                while current_off < smbios_data.len() {
                    if let Ok((header, next_off)) =
                        parsers::parse_smbios_structure(&smbios_data, current_off)
                    {
                        let mut label =
                            format!("Type {} (Handle 0x{:04X})", header.type_id, header.handle);
                        let type_name = match header.type_id {
                            0 => "BIOS Info",
                            1 => "System Info",
                            2 => "Baseboard",
                            3 => "Chassis",
                            4 => "Processor",
                            7 => "Cache Info",
                            8 => "Port Connector",
                            9 => "System Slots",
                            11 => "OEM Strings",
                            13 => "BIOS Language",
                            16 => "Memory Array",
                            17 => "Memory Device",
                            19 => "Memory Mapped",
                            32 => "Boot Info",
                            127 => "End-of-Table",
                            _ => "",
                        };
                        if !type_name.is_empty() {
                            label.push_str(" - ");
                            label.push_str(type_name);
                        }

                        smbios_list.push((
                            current_off,
                            header.type_id,
                            header.length,
                            header.handle,
                            label,
                        ));

                        if next_off <= current_off {
                            break;
                        }
                        current_off = next_off;
                    } else {
                        break;
                    }
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
            Ok(data) => {
                self.status = format!("Loaded ACPI {}", info.signature);
                self.update_cache(&data, "ACPI", &info.signature)
            }
            Err(e) => {
                self.cached_hex = format!("Error: {}", e);
                self.cached_parsed = format!("Error: {}", e);
                self.status = format!("ACPI load failed: {}", e);
            }
        }
    }

    /// Handles the selection of an SMBIOS structure and updates the detail views.
    fn select_smbios(&mut self, offset: usize, type_id: u8) {
        self.selected_item = Selection::Smbios(offset, type_id);
        if let Some(ref data) = self.smbios_data {
            if let Ok((_, next_off)) = parsers::parse_smbios_structure(data, offset) {
                let data_vec = data[offset..next_off].to_vec();
                self.status = format!("Loaded SMBIOS type {}", type_id);
                self.update_cache(&data_vec, "SMBIOS", &format!("Type {}", type_id));
            } else {
                self.status = "SMBIOS parse failed".to_string();
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
                        if let Some(fadt_info) = all_tables
                            .iter()
                            .find(|t| t.signature == "FACP" || t.signature == "FADT")
                        {
                            let data = if let Some(ref path) = fadt_info.registry_path {
                                api::get_acpi_table_by_path(path).ok()
                            } else {
                                api::get_system_firmware_table(api::SIG_ACPI, &fadt_info.signature)
                                    .ok()
                            };

                            if let Some(d) = data {
                                let refs = parsers::parse_fadt_references(&d);
                                for (a, s) in refs {
                                    addr_map.insert(a, s);
                                }
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

                out.push_str(&format!(
                    "Type {} (Handle 0x{:04X})\n",
                    header.type_id, header.handle
                ));
                out.push_str(&format!("Length: {}\n", header.length));
                out.push_str("====================\n");

                if let Some(details) =
                    parsers::parse_smbios_details(header.type_id, data, 0, header.length, &strings)
                {
                    for (k, v) in details {
                        out.push_str(&format!("{:25}: {}\n", k, v));
                    }
                } else if !strings.is_empty() {
                    out.push_str("Strings:\n");
                    for (i, s) in strings.iter().enumerate() {
                        out.push_str(&format!("  {}: {}\n", i + 1, s));
                    }
                } else {
                    out.push_str("No strings.\n");
                }
            }
        }
        self.cached_parsed = out;

        // Refresh search matches if query exists
        if !self.search_query.is_empty() {
            self.recompute_search_matches();
        }
    }

    /// Recomputes search match positions for the active text.
    fn recompute_search_matches(&mut self) {
        let text = match self.active_tab {
            Tab::Hex => &self.cached_hex,
            Tab::Parsed => &self.cached_parsed,
        }
        .to_lowercase();

        let query = self.search_query.to_lowercase();
        self.search_matches.clear();
        self.search_current = 0;
        if query.is_empty() {
            return;
        }

        let mut start = 0;
        while let Some(pos) = text[start..].find(&query) {
            self.search_matches.push(start + pos);
            start += pos + query.len().max(1);
        }
    }

    /// Advances search selection.
    fn step_search(&mut self, delta: isize) {
        if self.search_matches.is_empty() {
            return;
        }
        let len = self.search_matches.len();
        let cur = self.search_current as isize;
        let next = (cur + delta).rem_euclid(len as isize);
        self.search_current = next as usize;
    }

    /// Sanitizes a filename fragment for Windows.
    fn clean_filename_fragment(fragment: &str) -> String {
        let invalid = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        fragment
            .chars()
            .map(|c| if invalid.contains(&c) { '_' } else { c })
            .collect()
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

                match result {
                    Ok(data) => (
                        data,
                        format!(
                            "{}_{}.aml",
                            info.signature,
                            Self::clean_filename_fragment(info.table_id.trim())
                        ),
                    ),
                    Err(e) => {
                        rfd::MessageDialog::new()
                            .set_title("Export Error")
                            .set_description(format!("Failed to read table data: {}", e))
                            .set_level(rfd::MessageLevel::Error)
                            .show();
                        return;
                    }
                }
            }
            Selection::Smbios(off, tid) => {
                if let Some(ref smbios_data) = self.smbios_data {
                    if let Ok((_, next_off)) = parsers::parse_smbios_structure(smbios_data, *off) {
                        (
                            smbios_data[*off..next_off].to_vec(),
                            format!("smbios_type_{}.bin", tid),
                        )
                    } else {
                        rfd::MessageDialog::new()
                            .set_title("Export Error")
                            .set_description("Failed to parse SMBIOS structure.")
                            .set_level(rfd::MessageLevel::Error)
                            .show();
                        return;
                    }
                } else {
                    return;
                }
            }
            Selection::None => return,
        };

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .save_file()
        {
            match std::fs::File::create(&path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(&data) {
                        rfd::MessageDialog::new()
                            .set_title("Export Error")
                            .set_description(format!("Failed to write file: {}", e))
                            .set_level(rfd::MessageLevel::Error)
                            .show();
                    }
                }
                Err(e) => {
                    rfd::MessageDialog::new()
                        .set_title("Export Error")
                        .set_description(format!("Failed to create file: {}", e))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
        }
    }

    /// Opens a save file dialog to export the currently selected item's parsed view as a text file.
    fn export_parsed(&self) {
        let default_name = match &self.selected_item {
            Selection::Acpi(info) => format!(
                "{}_{}_parsed.txt",
                info.signature,
                Self::clean_filename_fragment(info.table_id.trim())
            ),
            Selection::Smbios(_, tid) => format!("smbios_type_{}_parsed.txt", tid),
            Selection::None => return,
        };

        if let Some(path) = rfd::FileDialog::new()
            .set_file_name(&default_name)
            .add_filter("Text Files", &["txt"])
            .save_file()
        {
            match std::fs::File::create(&path) {
                Ok(mut file) => {
                    if let Err(e) = file.write_all(self.cached_parsed.as_bytes()) {
                        rfd::MessageDialog::new()
                            .set_title("Export Error")
                            .set_description(format!("Failed to write file: {}", e))
                            .set_level(rfd::MessageLevel::Error)
                            .show();
                    }
                }
                Err(e) => {
                    rfd::MessageDialog::new()
                        .set_title("Export Error")
                        .set_description(format!("Failed to create file: {}", e))
                        .set_level(rfd::MessageLevel::Error)
                        .show();
                }
            }
        }
    }

    /// Opens a folder picker to export all discovered ACPI tables as individual binary files.
    fn export_all_acpi(&self) {
        if let Some(tables) = &self.acpi_tables {
            if let Some(folder) = rfd::FileDialog::new()
                .set_title("Select Folder to Export All ACPI Tables")
                .pick_folder()
            {
                let mut success_count = 0;
                let mut fail_count = 0;
                let mut errors: Vec<String> = Vec::new();

                for info in tables {
                    let result = if let Some(ref path) = info.registry_path {
                        api::get_acpi_table_by_path(path)
                    } else {
                        api::get_system_firmware_table(api::SIG_ACPI, &info.signature)
                    };

                    match result {
                        Ok(data) => {
                            let path = folder.join(format!(
                                "{}_{}.aml",
                                info.signature,
                                info.table_id.trim()
                            ));
                            match std::fs::File::create(&path) {
                                Ok(mut file) => {
                                    if file.write_all(&data).is_ok() {
                                        success_count += 1;
                                    } else {
                                        fail_count += 1;
                                        errors.push(format!("{}: write failed", info.signature));
                                    }
                                }
                                Err(_) => {
                                    fail_count += 1;
                                    errors.push(format!("{}: create failed", info.signature));
                                }
                            }
                        }
                        Err(_) => {
                            fail_count += 1;
                            errors.push(format!("{}: read failed", info.signature));
                        }
                    }
                }

                let message = if fail_count == 0 {
                    format!("Successfully exported {} tables.", success_count)
                } else {
                    format!(
                        "Exported {} tables, {} failed.\n\nErrors:\n{}",
                        success_count,
                        fail_count,
                        errors.join("\n")
                    )
                };

                rfd::MessageDialog::new()
                    .set_title("Export Complete")
                    .set_description(&message)
                    .set_level(if fail_count == 0 {
                        rfd::MessageLevel::Info
                    } else {
                        rfd::MessageLevel::Warning
                    })
                    .show();
            }
        }
    }

    /// Opens a save file dialog to export the entire raw SMBIOS information blob.
    fn export_full_smbios(&self) {
        if let Some(ref data) = self.smbios_data {
            if let Some(path) = rfd::FileDialog::new()
                .set_title("Save Full SMBIOS Data")
                .set_file_name("smbios_raw.bin")
                .save_file()
            {
                match std::fs::File::create(&path) {
                    Ok(mut file) => {
                        if let Err(e) = file.write_all(data) {
                            rfd::MessageDialog::new()
                                .set_title("Export Error")
                                .set_description(format!("Failed to write file: {}", e))
                                .set_level(rfd::MessageLevel::Error)
                                .show();
                        }
                    }
                    Err(e) => {
                        rfd::MessageDialog::new()
                            .set_title("Export Error")
                            .set_description(format!("Failed to create file: {}", e))
                            .set_level(rfd::MessageLevel::Error)
                            .show();
                    }
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
        let ascii_part: String = chunk
            .iter()
            .map(|&b| {
                if (32..127).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!(
            "{:04X}  {:<48}  {}\n",
            offset, hex_str, ascii_part
        ));
    }
    out
}

impl eframe::App for DumpApp {
    /// Main UI loop for the application.
    ///
    /// Defines the sidebar (table list), central panel (data view), search panel, and top toolbar.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme
        if self.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        egui::TopBottomPanel::top("admin_banner").show(ctx, |ui| {
            if !self.is_admin {
                ui.colored_label(
                    Color32::from_rgb(200, 50, 50),
                    "Running without Administrator privileges. Some firmware reads may fail.",
                );
            }
        });

        egui::SidePanel::left("sidebar_panel")
            .resizable(true)
            .default_width(320.0)
            .width_range(200.0..=500.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Firmware Tables");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let theme_icon = if self.dark_mode { "☀" } else { "🌙" };
                        let theme_tooltip = if self.dark_mode {
                            "Switch to Light Mode"
                        } else {
                            "Switch to Dark Mode"
                        };
                        if ui.button(theme_icon).on_hover_text(theme_tooltip).clicked() {
                            self.dark_mode = !self.dark_mode;
                        }
                    });
                });
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
                                    if !filter.is_empty() && !label.to_lowercase().contains(&filter)
                                    {
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
                                if let Some(h) = &self.smbios_header {
                                    ui.label(format!(
                                        "Version {}.{} | DMI rev {}",
                                        h._major_version, h._minor_version, h._dmi_revision
                                    ));
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("💾 Export Full Blob").clicked() {
                                        self.export_full_smbios();
                                    }
                                });
                                ui.separator();

                                let mut clicked_smbios = None;
                                for (offset, type_id, _length, _handle, label) in &self.smbios_list
                                {
                                    if !filter.is_empty() && !label.to_lowercase().contains(&filter)
                                    {
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
                        self.recompute_search_matches();
                    }
                    if ui
                        .selectable_label(self.active_tab == Tab::Parsed, "Parsed View")
                        .clicked()
                    {
                        self.active_tab = Tab::Parsed;
                        self.recompute_search_matches();
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

                        ui.separator();

                        // Clipboard copy button for current view
                        let has_data =
                            !self.cached_hex.is_empty() || !self.cached_parsed.is_empty();
                        if ui
                            .add_enabled(has_data, egui::Button::new("📋 Copy"))
                            .on_hover_text("Copy current view to clipboard")
                            .on_disabled_hover_text("Select an item first")
                            .clicked()
                        {
                            let text_to_copy = match self.active_tab {
                                Tab::Hex => &self.cached_hex,
                                Tab::Parsed => &self.cached_parsed,
                            };
                            self.copy_to_clipboard(ctx, text_to_copy);
                        }

                        if ui
                            .toggle_value(&mut self.search_panel_open, "🔍 Search (Ctrl+F)")
                            .clicked()
                        {}
                    });
                });
                ui.separator();

                // Search Bar
                if self.search_panel_open {
                    ui.horizontal(|ui| {
                        ui.label("Find:");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.search_query)
                                .hint_text("Enter text..."),
                        );
                        if self.search_panel_open {
                            response.request_focus();
                        }
                        if response.changed() {
                            self.recompute_search_matches();
                        }

                        if !self.search_query.is_empty() {
                            let total = self.search_matches.len();
                            let current = if total > 0 {
                                format!(" ({}/{})", self.search_current + 1, total)
                            } else {
                                "".to_string()
                            };
                            ui.label(format!("{} matches{}", total, current));
                            ui.add_enabled_ui(total > 0, |ui| {
                                if ui.button("Prev").clicked() {
                                    self.step_search(-1);
                                }
                                if ui.button("Next").clicked() {
                                    self.step_search(1);
                                }
                            });
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

                    // Highlight search matches in the active text
                    let matches = self.search_matches.clone();
                    let query = self.search_query.clone();
                    let active_idx = self.search_current;
                    let mut layouter =
                        move |ui: &egui::Ui, text: &str, wrap_width: f32| -> Arc<Galley> {
                            let mut job = egui::text::LayoutJob::default();
                            let default_fmt = TextFormat {
                                font_id: FontId::monospace(14.0),
                                color: ui.visuals().text_color(),
                                ..Default::default()
                            };

                            if query.is_empty() || matches.is_empty() {
                                job.append(text, 0.0, default_fmt.clone());
                            } else {
                                let query_len = query.len();
                                let mut cursor = 0;
                                for (i, pos) in matches.iter().enumerate() {
                                    if *pos > text.len() {
                                        break;
                                    }
                                    if cursor < *pos {
                                        job.append(&text[cursor..*pos], 0.0, default_fmt.clone());
                                    }
                                    let end = (*pos + query_len).min(text.len());
                                    let mut highlight_fmt = default_fmt.clone();
                                    highlight_fmt.background = if i == active_idx {
                                        Color32::from_rgb(80, 130, 210)
                                    } else {
                                        Color32::from_rgb(70, 70, 70)
                                    };
                                    highlight_fmt.color = Color32::WHITE;
                                    job.append(&text[*pos..end], 0.0, highlight_fmt);
                                    cursor = end;
                                }

                                if cursor < text.len() {
                                    job.append(&text[cursor..], 0.0, default_fmt);
                                }
                            }

                            job.wrap.max_width = wrap_width;
                            let galley: Arc<Galley> = ui.fonts(|f| f.layout_job(job));
                            galley
                        };

                    ui.add_sized(
                        ui.available_size(),
                        egui::TextEdit::multiline(text)
                            .font(egui::TextStyle::Monospace)
                            .lock_focus(true)
                            .layouter(&mut layouter),
                    );
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Status: {}", self.status));
            });
        });
    }
}
