use clap::Parser;
use crate::api;
use crate::parsers;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// List ACPI tables
    #[arg(long)]
    pub acpi: bool,

    /// Dump specific ACPI table by signature
    #[arg(long)]
    pub table: Option<String>,

    /// Dump SMBIOS structures
    #[arg(long)]
    pub smbios: bool,

    /// Launch GUI mode
    #[arg(long)]
    pub gui: bool,
}

pub fn run(args: Args) {
    if args.acpi {
        cmd_list_acpi();
    } else if let Some(table_sig) = args.table {
        cmd_dump_acpi(&table_sig);
    } else if args.smbios {
        cmd_smbios();
    } else {
        // If no args, print help? 
        // clap handles help with --help, but if no args provided maybe just print help.
        // We can just rely on clap for now, or print message.
        // Actually main will handle dispatching to GUI if no args? Or just follow flags.
        println!("No action specified. Use --help for usage.");
    }
}

fn hex_dump(data: &[u8]) {
    let length = 16;
    for (i, chunk) in data.chunks(length).enumerate() {
        let offset = i * length;
        let hex_part: Vec<String> = chunk.iter().map(|b| format!("{:02X}", b)).collect();
        let hex_str = hex_part.join(" ");
        
        let ascii_part: String = chunk.iter().map(|&b| {
            if b >= 32 && b < 127 {
                b as char
            } else {
                '.'
            }
        }).collect();

        println!("{:04X}  {:<48}  {}", offset, hex_str, ascii_part);
    }
}

fn cmd_list_acpi() {
    match api::enum_system_firmware_tables(api::SIG_ACPI) {
        Ok(tables) => {
            println!("ACPI Tables:");
            for t in tables {
                println!(" - {}", t);
            }
        }
        Err(e) => println!("[ERROR] {}", e),
    }
}

fn cmd_dump_acpi(signature: &str) {
    match api::get_system_firmware_table(api::SIG_ACPI, signature) {
        Ok(data) => {
            if data.is_empty() {
                println!("Table '{}' not found or empty.", signature);
                return;
            }
            
            // Parse Header
            match parsers::parse_acpi_header(&data) {
                Ok(header) => {
                    println!("Signature: {}", header.signature);
                    println!("Length:    {} bytes", header.length);
                    println!("OEM ID:    {}", header.oem_id);
                    println!("Table ID:  {}", header.oem_table_id);
                }
                Err(e) => println!("Error parsing header: {}", e),
            }

            println!("\n--- ACPI Table: {} ---", signature);
            hex_dump(&data);
        }
        Err(e) => println!("[ERROR] {}", e),
    }
}

fn cmd_smbios() {
    match api::get_smbios_data() {
        Ok(data) => {
            if data.is_empty() {
                println!("No SMBIOS data found.");
                return;
            }
            println!("Total Retrieved Data Size: {} bytes", data.len());

            let (offset, _header) = if let Some((header, off)) = parsers::parse_raw_smbios_data_header(&data) {
                println!("Found Windows SMBIOS Header:");
                println!("  Version: {}.{}", header.major_version, header.minor_version);
                println!("  DMI Revision: {}", header.dmi_revision);
                println!("  Table Length: {} bytes", header.length);
                (off, Some(header))
            } else {
                (0, None)
            };

            let parse_len = data.len() - offset;
            println!("Parsing SMBIOS structures starting at offset {} ({} bytes)...", offset, parse_len);

            let mut current_off = offset;
            let mut count = 0;

            while current_off < data.len() {
                match parsers::parse_smbios_structure(&data, current_off) {
                    Ok((header, next_off)) => {
                        count += 1;
                        let strings = parsers::get_smbios_strings(&data, current_off, header.length);
                        
                        println!("{}", "=".repeat(60));
                        let mut title = format!("Type {} (Handle 0x{:04X})", header.type_id, header.handle);
                        let type_name = match header.type_id {
                            0 => "BIOS Information",
                            1 => "System Information",
                            2 => "Baseboard Information",
                            3 => "Chassis Information",
                            4 => "Processor Information",
                            17 => "Memory Device",
                            _ => "",
                        };
                        if !type_name.is_empty() {
                            title.push_str(" - ");
                            title.push_str(type_name);
                        }
                        println!("{:^60}", title);
                        println!("{}", "=".repeat(60));

                        // Details
                        let details = parsers::parse_smbios_details(header.type_id, &data, current_off, header.length, &strings);
                        if let Some(fields) = details {
                            for (k, v) in fields {
                                println!("{:25}: {}", k, v);
                            }
                        } else {
                            println!("Length: 0x{:X}", header.length);
                            if !strings.is_empty() {
                                println!("{}", "-".repeat(60));
                                for (i, s) in strings.iter().enumerate() {
                                    println!("String {}: {}", i + 1, s);
                                }
                            }
                        }
                        println!("\n");

                        if next_off == current_off {
                             // Avoid infinite loop if no progress (should not happen with generic parser logic)
                             break;
                        }
                        current_off = next_off;
                    }
                    Err(_) => break, // End of list or error
                }
            }
            println!("Finished. Parsed {} structures.", count);
        }
        Err(e) => println!("[ERROR] {}", e),
    }
}
