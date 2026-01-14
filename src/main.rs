mod api;
mod parsers;
mod cli;
mod gui;

use clap::Parser;
use windows::Win32::UI::Shell::IsUserAnAdmin;

fn main() {
    let args = cli::Args::parse();

    // Check Admin
    let is_admin = unsafe { IsUserAnAdmin().as_bool() };
    if !is_admin {
        eprintln!("WARNING: Not running as Administrator. Firmware APIs will likely fail.");
        // If not admin, and potentially in CLI mode, we might want to pause? 
        // But if defaulting to GUI, the GUI will show emptiness or errors.
        // Let's just keep the warning.
    }

    // If no CLI args are specified, default to GUI
    let run_gui = args.gui || (!args.acpi && args.table.is_none() && !args.smbios);

    if run_gui {
        if let Err(e) = gui::run() {
            eprintln!("GUI Error: {}", e);
        }
    } else {
        cli::run(args);
    }
}
