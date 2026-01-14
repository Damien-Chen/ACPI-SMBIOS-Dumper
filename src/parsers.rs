use byteorder::{ByteOrder, LittleEndian};
use std::fmt;

#[derive(Debug)]
pub struct AcpiTableHeader {
    pub signature: String,
    pub length: u32,
    pub revision: u8,
    pub checksum: u8,
    pub oem_id: String,
    pub oem_table_id: String,
    pub oem_revision: u32,
    pub creator_id: String,
    pub creator_revision: u32,
}

pub fn parse_acpi_header(data: &[u8]) -> Result<AcpiTableHeader, String> {
    if data.len() < 36 {
        return Err("Data too short for ACPI header".into());
    }

    let signature = clean_str(&data[0..4]);
    let length = LittleEndian::read_u32(&data[4..8]);
    let revision = data[8];
    let checksum = data[9];
    let oem_id = clean_str(&data[10..16]);
    let oem_table_id = clean_str(&data[16..24]);
    let oem_revision = LittleEndian::read_u32(&data[24..28]);
    let creator_id = clean_str(&data[28..32]);
    let creator_revision = LittleEndian::read_u32(&data[32..36]);

    Ok(AcpiTableHeader {
        signature,
        length,
        revision,
        checksum,
        oem_id,
        oem_table_id,
        oem_revision,
        creator_id,
        creator_revision,
    })
}

fn clean_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_matches(char::from(0))
        .to_string()
}

#[derive(Debug)]
pub struct RawSMBIOSData {
    pub major_version: u8,
    pub minor_version: u8,
    pub dmi_revision: u8,
    pub length: u32,
}

pub fn parse_raw_smbios_data_header(data: &[u8]) -> Option<(RawSMBIOSData, usize)> {
    if data.len() < 8 {
        return None;
    }
    // struct RawSMBIOSData {
    //   BYTE  Used20CallingMethod;
    //   BYTE  SMBIOSMajorVersion;
    //   BYTE  SMBIOSMinorVersion;
    //   BYTE  DmiRevision;
    //   DWORD Length;
    //   BYTE  SMBIOSTableData[];
    // };
    let _u20 = data[0];
    let major = data[1];
    let minor = data[2];
    let dmi = data[3];
    let length = LittleEndian::read_u32(&data[4..8]);

    Some((
        RawSMBIOSData {
            major_version: major,
            minor_version: minor,
            dmi_revision: dmi,
            length,
        },
        8,
    ))
}

#[derive(Debug, Clone)]
pub struct SmbiosStructureHeader {
    pub type_id: u8,
    pub length: u8,
    pub handle: u16,
}

pub fn parse_smbios_structure(data: &[u8], offset: usize) -> Result<(SmbiosStructureHeader, usize), ()> {
    if offset + 4 > data.len() {
        return Err(());
    }

    let type_id = data[offset];
    let length = data[offset + 1];
    let handle = LittleEndian::read_u16(&data[offset + 2..offset + 4]);

    if length < 4 {
        return Err(());
    }

    let header = SmbiosStructureHeader {
        type_id,
        length,
        handle,
    };

    // Find end of structure (double null terminator)
    let formatted_end = offset + length as usize;
    let mut current = formatted_end;
    
    // Safety check for bounds
    while current + 1 < data.len() {
        if data[current] == 0 && data[current + 1] == 0 {
            return Ok((header, current + 2));
        }
        current += 1;
    }

    // If we hit end of data without double null, assume end of data is end of struct?
    // Usually double null is required. But if we are at the very end, returning len is ok.
    Ok((header, data.len()))
}

pub fn get_smbios_strings(data: &[u8], offset: usize, length: u8) -> Vec<String> {
    let mut strings = Vec::new();
    let str_start = offset + length as usize;
    
    if str_start >= data.len() {
        return strings;
    }

    let mut current_idx = str_start;
    while current_idx < data.len() {
        // Find next null
        match data[current_idx..].iter().position(|&b| b == 0) {
            Some(pos) => {
                let null_idx = current_idx + pos;
                if null_idx == current_idx {
                    // Empty string / end marker?
                    // Actually if we hit the second null of the double null, we stop.
                    // But here we are iterating strings.
                    // If we encounter an empty string (len 0), that might be the terminator.
                    break;
                }
                
                let s_bytes = &data[current_idx..null_idx];
                strings.push(String::from_utf8_lossy(s_bytes).to_string());
                
                current_idx = null_idx + 1;
                
                // If next byte is 0, we are done (double null)
                if current_idx < data.len() && data[current_idx] == 0 {
                    break;
                }
            }
            None => break, // No more nulls
        }
    }
    strings
}

pub fn get_string_by_index(strings: &[String], index: u8) -> String {
    if index == 0 {
        return "None".to_string();
    }
    let idx = index as usize;
    if idx > 0 && idx <= strings.len() {
        strings[idx - 1].clone()
    } else {
        format!("<Bad String Index: {}>", index)
    }
}

// SMBIOS Parsers

pub fn parse_smbios_details(type_id: u8, data: &[u8], offset: usize, header_len: u8, strings: &[String]) -> Option<Vec<(String, String)>> {
    match type_id {
        0 => Some(parse_type_0(data, offset, strings)),
        1 => Some(parse_type_1(data, offset, strings)),
        2 => Some(parse_type_2(data, offset, strings)),
        3 => Some(parse_type_3(data, offset, strings)),
        4 => Some(parse_type_4(data, offset, strings)),
        17 => Some(parse_type_17(data, offset, strings)),
        _ => None,
    }
}

fn parse_type_0(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    // Offset 0x04: Vendor String Index
    if offset + 0x09 < data.len() {
        let vendor_idx = data[offset + 0x04];
        let ver_idx = data[offset + 0x05];
        let date_idx = data[offset + 0x08];
        let rom_size_enc = data[offset + 0x09];

        info.push(("Vendor".to_string(), get_string_by_index(strings, vendor_idx)));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push(("Release Date".to_string(), get_string_by_index(strings, date_idx)));

        let size = if rom_size_enc == 0xFF {
            // Extended calculation omitted for brevity, logic similar to Python
            "Extended".to_string()
        } else {
            let kb = (rom_size_enc as u32 + 1) * 64;
            format!("{} KB", kb)
        };
        info.push(("ROM Size".to_string(), size));
    }
    info
}

fn parse_type_1(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x18 <= data.len() { // Check length for UUID
         let man_idx = data[offset + 0x04];
         let prod_idx = data[offset + 0x05];
         let ver_idx = data[offset + 0x06];
         let ser_idx = data[offset + 0x07];

         info.push(("Manufacturer".to_string(), get_string_by_index(strings, man_idx)));
         info.push(("Product Name".to_string(), get_string_by_index(strings, prod_idx)));
         info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
         info.push(("Serial Number".to_string(), get_string_by_index(strings, ser_idx)));

         // UUID at 0x08 (16 bytes)
         let uuid_bytes = &data[offset + 0x08..offset + 0x18];
         // Try to parse using uuid crate
         // uuid crate expects bytes. construct from slice.
         if let Ok(u) = uuid::Uuid::from_slice_le(uuid_bytes) { // SMBIOS 2.6+ uses Little Endian for first 3 fields
             info.push(("UUID".to_string(), u.to_string().to_uppercase()));
         } else {
             info.push(("UUID".to_string(), hex::encode(uuid_bytes).to_uppercase()));
         }
    }
    info
}

fn parse_type_2(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
     if offset + 0x08 < data.len() {
         let man_idx = data[offset + 0x04];
         let prod_idx = data[offset + 0x05];
         let ver_idx = data[offset + 0x06];
         let ser_idx = data[offset + 0x07];
         // asset_idx is at 0x08, check length if needed (Baseboard usually has it)
         let asset_idx = if offset + 0x08 < data.len() { data[offset + 0x08] } else { 0 };

         info.push(("Manufacturer".to_string(), get_string_by_index(strings, man_idx)));
         info.push(("Product Name".to_string(), get_string_by_index(strings, prod_idx)));
         info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
         info.push(("Serial Number".to_string(), get_string_by_index(strings, ser_idx)));
         info.push(("Asset Tag".to_string(), get_string_by_index(strings, asset_idx)));
    }
    info
}

fn parse_type_3(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x07 < data.len() {
        let man_idx = data[offset + 0x04];
        let type_code = data[offset + 0x05];
        let ver_idx = data[offset + 0x06];
        let ser_idx = data[offset + 0x07];

        info.push(("Manufacturer".to_string(), get_string_by_index(strings, man_idx)));
        info.push(("Type".to_string(), format!("0x{:02X}", type_code)));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push(("Serial Number".to_string(), get_string_by_index(strings, ser_idx)));
    }
    info
}

fn parse_type_4(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x10 < data.len() {
        let sock_idx = data[offset + 0x04];
        let type_enum = data[offset + 0x05];
        let man_idx = data[offset + 0x07];
        let ver_idx = data[offset + 0x10];

        info.push(("Socket Designator".to_string(), get_string_by_index(strings, sock_idx)));
        info.push(("Processor Type".to_string(), format!("0x{:02X}", type_enum)));
        info.push(("Manufacturer".to_string(), get_string_by_index(strings, man_idx)));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));

        if offset + 0x25 < data.len() {
            let core_count = data[offset + 0x23];
            let thread_count = data[offset + 0x25];
            info.push(("Core Count".to_string(), core_count.to_string()));
            info.push(("Thread Count".to_string(), thread_count.to_string()));
        }
    }
    info
}

fn parse_type_17(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x1B < data.len() {
        let total_width = LittleEndian::read_u16(&data[offset + 0x08..offset + 0x0A]);
        let data_width = LittleEndian::read_u16(&data[offset + 0x0A..offset + 0x0C]);
        let size = LittleEndian::read_u16(&data[offset + 0x0C..offset + 0x0E]);
        let speed = LittleEndian::read_u16(&data[offset + 0x15..offset + 0x17]);
        
        let man_idx = data[offset + 0x17];
        let ser_idx = data[offset + 0x18];
        let asset_idx = data[offset + 0x19];
        let part_idx = data[offset + 0x1A];
        
        let dev_idx = data[offset + 0x10];
        let bank_idx = data[offset + 0x11];

        info.push(("Device Locator".to_string(), get_string_by_index(strings, dev_idx)));
        info.push(("Bank Locator".to_string(), get_string_by_index(strings, bank_idx)));

        if size == 0xFFFF {
            info.push(("Size".to_string(), "Unknown / Extended".to_string()));
        } else if size == 0 {
            info.push(("Size".to_string(), "No Module Installed".to_string()));
        } else {
            if size & 0x8000 != 0 {
                let s_val = size & 0x7FFF;
                info.push(("Size".to_string(), format!("{} KB", s_val)));
            } else {
                info.push(("Size".to_string(), format!("{} MB", size)));
            }
        }

        info.push(("Speed".to_string(), if speed != 0 { format!("{} MT/s", speed) } else { "Unknown".to_string() }));
        info.push(("Manufacturer".to_string(), get_string_by_index(strings, man_idx)));
        info.push(("Serial Number".to_string(), get_string_by_index(strings, ser_idx)));
        info.push(("Asset Tag".to_string(), get_string_by_index(strings, asset_idx)));
        info.push(("Part Number".to_string(), get_string_by_index(strings, part_idx)));
        
        info.push(("Total Width".to_string(), format!("{} bits", total_width)));
        info.push(("Data Width".to_string(), format!("{} bits", data_width)));
    }
    info
}
