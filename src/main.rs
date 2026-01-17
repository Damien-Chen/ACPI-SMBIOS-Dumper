#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// The `api` module handles low-level firmware table retrieval from the OS.
mod api;
/// The `parsers` module containing logic to interpret raw bytes for ACPI and SMBIOS.
mod parsers;
/// The `gui` module manages the application's graphical user interface.
mod gui;

use windows::Win32::UI::Shell::IsUserAnAdmin;

/// The entry point of the application.
///
/// It performs a preliminary check for administrator privileges, which are required
/// to access firmware tables on Windows, and then launches the GUI.
fn main() {
    // Check if the application is running with Administrator privileges
    let is_admin = unsafe { IsUserAnAdmin().as_bool() };
    if !is_admin {
        eprintln!("WARNING: Not running as Administrator. Firmware APIs will likely fail.");
    }

    // Launch the Graphical User Interface
    if let Err(e) = gui::run() {
        eprintln!("GUI Error: {}", e);
    }
}
