use byteorder::{ByteOrder, LittleEndian};

/// Standard ACPI table header structure (36 bytes).
///
/// Every ACPI table begins with this common header, which contains the table signature,
/// length, and OEM identification information.
#[derive(Debug)]
pub struct AcpiTableHeader {
    /// The 4-character ASCII signature (e.g., "FACP", "APIC").
    pub signature: String,
    /// The total length of the table, including the header.
    pub length: u32,
    /// The revision of the table structure.
    pub _revision: u8,
    /// The checksum of the entire table.
    pub _checksum: u8,
    /// The OEM ID string (6 characters).
    pub oem_id: String,
    /// The OEM Table ID string (8 characters).
    pub oem_table_id: String,
    /// The OEM revision number.
    pub _oem_revision: u32,
    /// The ASL compiler Vendor ID.
    pub _creator_id: String,
    /// The ASL compiler revision number.
    pub _creator_revision: u32,
}

/// Parses a 36-byte ACPI header from a raw byte slice.
///
/// # Arguments
/// * `data` - The raw byte slice containing the ACPI table.
///
/// # Returns
/// A `Result` containing the parsed `AcpiTableHeader` or an error string if the data is too short.
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
        _revision: revision,
        _checksum: checksum,
        oem_id,
        oem_table_id,
        _oem_revision: oem_revision,
        _creator_id: creator_id,
        _creator_revision: creator_revision,
    })
}

/// Extracts DSDT and FACS physical addresses from a Fixed ACPI Description Table (FADT/FACP).
///
/// # Arguments
/// * `data` - The raw binary data of the FADT table.
///
/// # Returns
/// A vector of tuples containing the physical address and the signature of the referenced table.
pub fn parse_fadt_references(data: &[u8]) -> Vec<(u64, String)> {
    let mut refs = Vec::new();
    if data.len() < 36 {
        return refs;
    }

    let sig = clean_str(&data[0..4]);
    if sig != "FACP" {
        return refs;
    }

    // FADT (FACP) Structure
    // FACS address at offset 36 (32-bit) or 132 (64-bit)
    // DSDT address at offset 40 (32-bit) or 140 (64-bit)

    let facs_32 = LittleEndian::read_u32(&data[36..40]) as u64;
    let dsdt_32 = LittleEndian::read_u32(&data[40..44]) as u64;

    if data.len() >= 148 {
        let facs_64 = LittleEndian::read_u64(&data[132..140]);
        let dsdt_64 = LittleEndian::read_u64(&data[140..148]);

        let facs = if facs_64 != 0 { facs_64 } else { facs_32 };
        let dsdt = if dsdt_64 != 0 { dsdt_64 } else { dsdt_32 };

        if facs != 0 {
            refs.push((facs, "FACS".to_string()));
        }
        if dsdt != 0 {
            refs.push((dsdt, "DSDT".to_string()));
        }
    } else {
        if facs_32 != 0 {
            refs.push((facs_32, "FACS".to_string()));
        }
        if dsdt_32 != 0 {
            refs.push((dsdt_32, "DSDT".to_string()));
        }
    }

    refs
}

/// Cleans a byte slice by converting it to a lossy UTF-8 string and trimming null terminators.
fn clean_str(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim_matches(char::from(0))
        .to_string()
}

/// Parses an eXtended System Description Table (XSDT) to extract 64-bit physical address entries.
///
/// # Arguments
/// * `data` - The raw binary data of the XSDT table.
/// * `addr_map` - A map of addresses to known signatures used for labeling entries.
///
/// # Returns
/// `Option<Vec<(usize, u64, String)>>` containing the index, address, and label for each entry.
pub fn parse_xsdt_entries(
    data: &[u8],
    addr_map: &std::collections::HashMap<u64, String>,
) -> Option<Vec<(usize, u64, String)>> {
    if data.len() < 36 {
        return None;
    }

    let sig = clean_str(&data[0..4]);
    if sig != "XSDT" {
        return None;
    }

    let table_len = LittleEndian::read_u32(&data[4..8]) as usize;
    if table_len > data.len() || table_len < 36 {
        return None;
    }

    // XSDT entries start at offset 36 (after standard header)
    // Each entry is 8 bytes (64-bit pointer)
    let entries_data = &data[36..table_len];
    let entry_count = entries_data.len() / 8;

    let mut entries = Vec::new();
    for i in 0..entry_count {
        let offset = i * 8;
        if offset + 8 > entries_data.len() {
            break;
        }
        let addr = LittleEndian::read_u64(&entries_data[offset..offset + 8]);

        let label = addr_map
            .get(&addr)
            .cloned()
            .unwrap_or_else(|| format!("Entry{}", i));
        entries.push((i, addr, label));
    }

    Some(entries)
}

/// Metadata for the raw SMBIOS data structure as retrieved from Windows.
#[derive(Debug)]
pub struct RawSMBIOSData {
    pub _major_version: u8,
    pub _minor_version: u8,
    pub _dmi_revision: u8,
    pub _length: u32,
}

/// Parses the header of the raw SMBIOS data blob returned by Windows APIs.
///
/// # Arguments
/// * `data` - The raw SMBIOS data buffer.
///
/// # Returns
/// `Option<(RawSMBIOSData, usize)>` containing the parsed header and the size of the header in bytes.
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
            _major_version: major,
            _minor_version: minor,
            _dmi_revision: dmi,
            _length: length,
        },
        8,
    ))
}

/// Header for an individual SMBIOS structure.
#[derive(Debug, Clone)]
pub struct SmbiosStructureHeader {
    /// The SMBIOS type ID (e.g., 0 for BIOS, 1 for System).
    pub type_id: u8,
    /// The length of the formatted portion of the structure.
    pub length: u8,
    /// The unique handle for this structure instance.
    pub handle: u16,
}

/// Parses a single SMBIOS structure header and calculated its total size (including strings).
///
/// # Arguments
/// * `data` - The raw SMBIOS data buffer.
/// * `offset` - The current offset into the buffer.
///
/// # Returns
/// `Result` containing the header and the next offset (end of the structure).
pub fn parse_smbios_structure(
    data: &[u8],
    offset: usize,
) -> Result<(SmbiosStructureHeader, usize), ()> {
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

    // Find end of structure (terminated by a double null: 00 00)
    let formatted_end = offset + length as usize;
    let mut current = formatted_end;

    while current + 1 < data.len() {
        if data[current] == 0 && data[current + 1] == 0 {
            return Ok((header, current + 2));
        }
        current += 1;
    }

    Ok((header, data.len()))
}

/// Extracts the string pool following the formatted portion of an SMBIOS structure.
///
/// # Arguments
/// * `data` - The raw SMBIOS data buffer.
/// * `offset` - The starting offset of the structure's formatted portion.
/// * `length` - The length of the formatted portion.
///
/// # Returns
/// A vector of strings extracted from the string pool.
pub fn get_smbios_strings(data: &[u8], offset: usize, length: u8) -> Vec<String> {
    let mut strings = Vec::new();
    let str_start = offset + length as usize;

    if str_start >= data.len() {
        return strings;
    }

    let mut current_idx = str_start;
    while current_idx < data.len() {
        match data[current_idx..].iter().position(|&b| b == 0) {
            Some(pos) => {
                let null_idx = current_idx + pos;
                if null_idx == current_idx {
                    break;
                }

                let s_bytes = &data[current_idx..null_idx];
                strings.push(String::from_utf8_lossy(s_bytes).to_string());

                current_idx = null_idx + 1;

                if current_idx < data.len() && data[current_idx] == 0 {
                    break;
                }
            }
            None => break,
        }
    }
    strings
}

/// Retrieves a string from the SMBIOS string pool by its 1-based index.
///
/// # Arguments
/// * `strings` - The list of strings extracted from the structure.
/// * `index` - The 1-based index of the string (0 means "None").
///
/// # Returns
/// The requested string, or a placeholder if the index is invalid.
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

/// Dispatches raw SMBIOS structure data to specific type parsers to get human-readable key-value pairs.
///
/// # Arguments
/// * `type_id` - The SMBIOS structure type (0, 1, 2, etc.).
/// * `data` - The raw buffer.
/// * `offset` - Starting offset of the structure.
/// * `strings` - The extracted strings for this structure.
///
/// # Returns
/// `Option<Vec<(String, String)>>` containing field names and values.
pub fn parse_smbios_details(
    type_id: u8,
    data: &[u8],
    offset: usize,
    _header_len: u8,
    strings: &[String],
) -> Option<Vec<(String, String)>> {
    match type_id {
        0 => Some(parse_type_0(data, offset, strings)),
        1 => Some(parse_type_1(data, offset, strings)),
        2 => Some(parse_type_2(data, offset, strings)),
        3 => Some(parse_type_3(data, offset, strings)),
        4 => Some(parse_type_4(data, offset, strings)),
        7 => Some(parse_type_7(data, offset, strings)),
        8 => Some(parse_type_8(data, offset, strings)),
        9 => Some(parse_type_9(data, offset, strings)),
        11 => Some(parse_type_11(data, offset, strings)),
        13 => Some(parse_type_13(data, offset, strings)),
        16 => Some(parse_type_16(data, offset, strings)),
        17 => Some(parse_type_17(data, offset, strings)),
        19 => Some(parse_type_19(data, offset, strings)),
        32 => Some(parse_type_32(data, offset, strings)),
        127 => Some(parse_type_127(data, offset, strings)),
        _ => None,
    }
}

/// Parser for SMBIOS Type 0: BIOS Information.
fn parse_type_0(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x09 < data.len() {
        let vendor_idx = data[offset + 0x04];
        let ver_idx = data[offset + 0x05];
        let date_idx = data[offset + 0x08];
        let rom_size_enc = data[offset + 0x09];

        info.push((
            "Vendor".to_string(),
            get_string_by_index(strings, vendor_idx),
        ));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push((
            "Release Date".to_string(),
            get_string_by_index(strings, date_idx),
        ));

        let size = if rom_size_enc == 0xFF {
            "Extended".to_string()
        } else {
            let kb = (rom_size_enc as u32 + 1) * 64;
            format!("{} KB", kb)
        };
        info.push(("ROM Size".to_string(), size));
    }
    info
}

/// Parser for SMBIOS Type 1: System Information.
fn parse_type_1(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x18 <= data.len() {
        let man_idx = data[offset + 0x04];
        let prod_idx = data[offset + 0x05];
        let ver_idx = data[offset + 0x06];
        let ser_idx = data[offset + 0x07];

        info.push((
            "Manufacturer".to_string(),
            get_string_by_index(strings, man_idx),
        ));
        info.push((
            "Product Name".to_string(),
            get_string_by_index(strings, prod_idx),
        ));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push((
            "Serial Number".to_string(),
            get_string_by_index(strings, ser_idx),
        ));

        // UUID at 0x08 (16 bytes)
        let uuid_bytes = &data[offset + 0x08..offset + 0x18];
        if let Ok(u) = uuid::Uuid::from_slice_le(uuid_bytes) {
            info.push(("UUID".to_string(), u.to_string().to_uppercase()));
        } else {
            info.push(("UUID".to_string(), hex::encode(uuid_bytes).to_uppercase()));
        }
    }
    info
}

/// Parser for SMBIOS Type 2: Baseboard (or Module) Information.
fn parse_type_2(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x08 < data.len() {
        let man_idx = data[offset + 0x04];
        let prod_idx = data[offset + 0x05];
        let ver_idx = data[offset + 0x06];
        let ser_idx = data[offset + 0x07];
        let asset_idx = if offset + 0x08 < data.len() {
            data[offset + 0x08]
        } else {
            0
        };

        info.push((
            "Manufacturer".to_string(),
            get_string_by_index(strings, man_idx),
        ));
        info.push((
            "Product Name".to_string(),
            get_string_by_index(strings, prod_idx),
        ));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push((
            "Serial Number".to_string(),
            get_string_by_index(strings, ser_idx),
        ));
        info.push((
            "Asset Tag".to_string(),
            get_string_by_index(strings, asset_idx),
        ));
    }
    info
}

/// Parser for SMBIOS Type 3: System Enclosure or Chassis Information.
fn parse_type_3(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x07 < data.len() {
        let man_idx = data[offset + 0x04];
        let type_code = data[offset + 0x05];
        let ver_idx = data[offset + 0x06];
        let ser_idx = data[offset + 0x07];

        info.push((
            "Manufacturer".to_string(),
            get_string_by_index(strings, man_idx),
        ));
        info.push(("Type".to_string(), format!("0x{:02X}", type_code)));
        info.push(("Version".to_string(), get_string_by_index(strings, ver_idx)));
        info.push((
            "Serial Number".to_string(),
            get_string_by_index(strings, ser_idx),
        ));
    }
    info
}

/// Parser for SMBIOS Type 4: Processor Information.
fn parse_type_4(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x10 < data.len() {
        let sock_idx = data[offset + 0x04];
        let type_enum = data[offset + 0x05];
        let man_idx = data[offset + 0x07];
        let ver_idx = data[offset + 0x10];

        info.push((
            "Socket Designator".to_string(),
            get_string_by_index(strings, sock_idx),
        ));
        info.push(("Processor Type".to_string(), format!("0x{:02X}", type_enum)));
        info.push((
            "Manufacturer".to_string(),
            get_string_by_index(strings, man_idx),
        ));
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

/// Parser for SMBIOS Type 17: Memory Device Information.
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

        info.push((
            "Device Locator".to_string(),
            get_string_by_index(strings, dev_idx),
        ));
        info.push((
            "Bank Locator".to_string(),
            get_string_by_index(strings, bank_idx),
        ));

        if size == 0xFFFF {
            info.push(("Size".to_string(), "Unknown / Extended".to_string()));
        } else if size == 0 {
            info.push(("Size".to_string(), "No Module Installed".to_string()));
        } else if size & 0x8000 != 0 {
            let s_val = size & 0x7FFF;
            info.push(("Size".to_string(), format!("{} KB", s_val)));
        } else {
            info.push(("Size".to_string(), format!("{} MB", size)));
        }

        info.push((
            "Speed".to_string(),
            if speed != 0 {
                format!("{} MT/s", speed)
            } else {
                "Unknown".to_string()
            },
        ));
        info.push((
            "Manufacturer".to_string(),
            get_string_by_index(strings, man_idx),
        ));
        info.push((
            "Serial Number".to_string(),
            get_string_by_index(strings, ser_idx),
        ));
        info.push((
            "Asset Tag".to_string(),
            get_string_by_index(strings, asset_idx),
        ));
        info.push((
            "Part Number".to_string(),
            get_string_by_index(strings, part_idx),
        ));

        info.push(("Total Width".to_string(), format!("{} bits", total_width)));
        info.push(("Data Width".to_string(), format!("{} bits", data_width)));
    }
    info
}

/// Parser for SMBIOS Type 7: Cache Information.
fn parse_type_7(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x0F < data.len() {
        let sock_idx = data[offset + 0x04];
        let cfg = LittleEndian::read_u16(&data[offset + 0x05..offset + 0x07]);
        let max_size = LittleEndian::read_u16(&data[offset + 0x07..offset + 0x09]);
        let inst_size = LittleEndian::read_u16(&data[offset + 0x09..offset + 0x0B]);
        let speed = data[offset + 0x0F];

        info.push((
            "Socket Designator".to_string(),
            get_string_by_index(strings, sock_idx),
        ));
        info.push(("Configuration".to_string(), format!("0x{:04X}", cfg)));

        let parse_size = |s: u16| {
            if s == 0 {
                return "None".to_string();
            }
            let val = s & 0x7FFF;
            if s & 0x8000 != 0 {
                format!("{} KB", val * 64)
            } else {
                format!("{} KB", val)
            }
        };

        info.push(("Maximum Cache Size".to_string(), parse_size(max_size)));
        info.push(("Installed Size".to_string(), parse_size(inst_size)));
        info.push((
            "Speed".to_string(),
            if speed != 0 {
                format!("{} ns", speed)
            } else {
                "Unknown".to_string()
            },
        ));

        if offset + 0x12 < data.len() {
            let err_corr = data[offset + 0x10];
            let sys_type = data[offset + 0x11];
            let assoc = data[offset + 0x12];
            info.push((
                "Error Correction".to_string(),
                format!("0x{:02X}", err_corr),
            ));
            info.push((
                "System Cache Type".to_string(),
                format!("0x{:02X}", sys_type),
            ));
            info.push(("Associativity".to_string(), format!("0x{:02X}", assoc)));
        }
    }
    info
}

/// Parser for SMBIOS Type 9: System Slots Information.
fn parse_type_9(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x09 < data.len() {
        let name_idx = data[offset + 0x04];
        let slot_type = data[offset + 0x05];
        let bus_width = data[offset + 0x06];
        let usage = data[offset + 0x07];
        let len = data[offset + 0x08];
        let id = LittleEndian::read_u16(&data[offset + 0x09..offset + 0x0B]);

        info.push((
            "Slot Designator".to_string(),
            get_string_by_index(strings, name_idx),
        ));
        info.push(("Slot Type".to_string(), format!("0x{:02X}", slot_type)));
        info.push(("Data Bus Width".to_string(), format!("0x{:02X}", bus_width)));
        info.push(("Current Usage".to_string(), format!("0x{:02X}", usage)));
        info.push(("Slot Length".to_string(), format!("0x{:02X}", len)));
        info.push(("Slot ID".to_string(), format!("0x{:04X}", id)));
    }
    info
}

/// Parser for SMBIOS Type 11: OEM Strings Information.
fn parse_type_11(_data: &[u8], _offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    for (i, s) in strings.iter().enumerate() {
        info.push((format!("String {}", i + 1), s.clone()));
    }
    info
}

/// Parser for SMBIOS Type 32: System Boot Information.
fn parse_type_32(data: &[u8], offset: usize, _strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x0A < data.len() {
        let status = data[offset + 0x0A];
        info.push(("Boot Status".to_string(), format!("0x{:02X}", status)));

        let status_msg = match status {
            0 => "No errors detected",
            1 => "No bootable media",
            2 => "Normal boot",
            3 => "User-requested boot",
            4 => "System-requested boot",
            5 => "Kernel panic",
            6 => "Recovery mode",
            _ => "Other / Unknown",
        };
        info.push(("Status Description".to_string(), status_msg.to_string()));
    }
    info
}

/// Parser for SMBIOS Type 8: Port Connector Information.
fn parse_type_8(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x09 <= data.len() {
        let int_ref_idx = data[offset + 0x04];
        let int_conn_type = data[offset + 0x05];
        let ext_ref_idx = data[offset + 0x06];
        let ext_conn_type = data[offset + 0x07];
        let port_type = data[offset + 0x08];

        info.push((
            "Internal Reference".to_string(),
            get_string_by_index(strings, int_ref_idx),
        ));
        info.push((
            "Internal Connector Type".to_string(),
            connector_type_str(int_conn_type),
        ));
        info.push((
            "External Reference".to_string(),
            get_string_by_index(strings, ext_ref_idx),
        ));
        info.push((
            "External Connector Type".to_string(),
            connector_type_str(ext_conn_type),
        ));
        info.push(("Port Type".to_string(), port_type_str(port_type)));
    }
    info
}

/// Returns human-readable connector type string.
fn connector_type_str(code: u8) -> String {
    match code {
        0x00 => "None".to_string(),
        0x01 => "Centronics".to_string(),
        0x02 => "Mini Centronics".to_string(),
        0x03 => "Proprietary".to_string(),
        0x04 => "DB-25 pin male".to_string(),
        0x05 => "DB-25 pin female".to_string(),
        0x06 => "DB-15 pin male".to_string(),
        0x07 => "DB-15 pin female".to_string(),
        0x08 => "DB-9 pin male".to_string(),
        0x09 => "DB-9 pin female".to_string(),
        0x0A => "RJ-11".to_string(),
        0x0B => "RJ-45".to_string(),
        0x0C => "50-pin MiniSCSI".to_string(),
        0x0D => "Mini-DIN".to_string(),
        0x0E => "Micro-DIN".to_string(),
        0x0F => "PS/2".to_string(),
        0x10 => "Infrared".to_string(),
        0x11 => "HP-HIL".to_string(),
        0x12 => "Access Bus (USB)".to_string(),
        0x13 => "SSA SCSI".to_string(),
        0x14 => "Circular DIN-8 male".to_string(),
        0x15 => "Circular DIN-8 female".to_string(),
        0x16 => "On Board IDE".to_string(),
        0x17 => "On Board Floppy".to_string(),
        0x18 => "9-pin Dual Inline (pin 10 cut)".to_string(),
        0x19 => "25-pin Dual Inline (pin 26 cut)".to_string(),
        0x1A => "50-pin Dual Inline".to_string(),
        0x1B => "68-pin Dual Inline".to_string(),
        0x1C => "On Board Sound Input from CD-ROM".to_string(),
        0x1D => "Mini-Centronics Type-14".to_string(),
        0x1E => "Mini-Centronics Type-26".to_string(),
        0x1F => "Mini-jack (headphones)".to_string(),
        0x20 => "BNC".to_string(),
        0x21 => "1394".to_string(),
        0x22 => "SAS/SATA Plug Receptacle".to_string(),
        0x23 => "USB Type-C Receptacle".to_string(),
        0xA0 => "PC-98".to_string(),
        0xA1 => "PC-98Hireso".to_string(),
        0xA2 => "PC-H98".to_string(),
        0xA3 => "PC-98Note".to_string(),
        0xA4 => "PC-98Full".to_string(),
        0xFF => "Other".to_string(),
        _ => format!("Unknown (0x{:02X})", code),
    }
}

/// Returns human-readable port type string.
fn port_type_str(code: u8) -> String {
    match code {
        0x00 => "None".to_string(),
        0x01 => "Parallel Port XT/AT Compatible".to_string(),
        0x02 => "Parallel Port PS/2".to_string(),
        0x03 => "Parallel Port ECP".to_string(),
        0x04 => "Parallel Port EPP".to_string(),
        0x05 => "Parallel Port ECP/EPP".to_string(),
        0x06 => "Serial Port XT/AT Compatible".to_string(),
        0x07 => "Serial Port 16450 Compatible".to_string(),
        0x08 => "Serial Port 16550 Compatible".to_string(),
        0x09 => "Serial Port 16550A Compatible".to_string(),
        0x0A => "SCSI Port".to_string(),
        0x0B => "MIDI Port".to_string(),
        0x0C => "Joy Stick Port".to_string(),
        0x0D => "Keyboard Port".to_string(),
        0x0E => "Mouse Port".to_string(),
        0x0F => "SSA SCSI".to_string(),
        0x10 => "USB".to_string(),
        0x11 => "FireWire (IEEE P1394)".to_string(),
        0x12 => "PCMCIA Type I".to_string(),
        0x13 => "PCMCIA Type II".to_string(),
        0x14 => "PCMCIA Type III".to_string(),
        0x15 => "Cardbus".to_string(),
        0x16 => "Access Bus Port".to_string(),
        0x17 => "SCSI II".to_string(),
        0x18 => "SCSI Wide".to_string(),
        0x19 => "PC-98".to_string(),
        0x1A => "PC-98-Hireso".to_string(),
        0x1B => "PC-H98".to_string(),
        0x1C => "Video Port".to_string(),
        0x1D => "Audio Port".to_string(),
        0x1E => "Modem Port".to_string(),
        0x1F => "Network Port".to_string(),
        0x20 => "SATA".to_string(),
        0x21 => "SAS".to_string(),
        0x22 => "MFDP (Multi-Function Display Port)".to_string(),
        0x23 => "Thunderbolt".to_string(),
        0xA0 => "8251 Compatible".to_string(),
        0xA1 => "8251 FIFO Compatible".to_string(),
        0xFF => "Other".to_string(),
        _ => format!("Unknown (0x{:02X})", code),
    }
}

/// Parser for SMBIOS Type 13: BIOS Language Information.
fn parse_type_13(data: &[u8], offset: usize, strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x16 <= data.len() {
        let installable_langs = data[offset + 0x04];
        let flags = data[offset + 0x05];
        let current_lang_idx = data[offset + 0x15];

        info.push((
            "Installable Languages".to_string(),
            installable_langs.to_string(),
        ));
        info.push((
            "Format".to_string(),
            if flags & 0x01 != 0 {
                "Abbreviated"
            } else {
                "Long"
            }
            .to_string(),
        ));
        info.push((
            "Current Language".to_string(),
            get_string_by_index(strings, current_lang_idx),
        ));

        if !strings.is_empty() {
            info.push(("Available Languages".to_string(), strings.join(", ")));
        }
    }
    info
}

/// Parser for SMBIOS Type 16: Physical Memory Array.
fn parse_type_16(data: &[u8], offset: usize, _strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x0F <= data.len() {
        let location = data[offset + 0x04];
        let use_code = data[offset + 0x05];
        let err_correction = data[offset + 0x06];
        let max_capacity = LittleEndian::read_u32(&data[offset + 0x07..offset + 0x0B]);
        let err_info_handle = LittleEndian::read_u16(&data[offset + 0x0B..offset + 0x0D]);
        let num_devices = LittleEndian::read_u16(&data[offset + 0x0D..offset + 0x0F]);

        info.push(("Location".to_string(), memory_array_location_str(location)));
        info.push(("Use".to_string(), memory_array_use_str(use_code)));
        info.push((
            "Error Correction".to_string(),
            memory_error_correction_str(err_correction),
        ));

        if max_capacity == 0x80000000 {
            if offset + 0x17 <= data.len() {
                let ext_max = LittleEndian::read_u64(&data[offset + 0x0F..offset + 0x17]);
                info.push((
                    "Maximum Capacity".to_string(),
                    format!("{} GB", ext_max / (1024 * 1024)),
                ));
            } else {
                info.push((
                    "Maximum Capacity".to_string(),
                    "Extended (>2TB)".to_string(),
                ));
            }
        } else {
            info.push((
                "Maximum Capacity".to_string(),
                format!("{} KB", max_capacity),
            ));
        }

        if err_info_handle != 0xFFFE && err_info_handle != 0xFFFF {
            info.push((
                "Error Info Handle".to_string(),
                format!("0x{:04X}", err_info_handle),
            ));
        } else {
            info.push(("Error Info Handle".to_string(), "Not Provided".to_string()));
        }

        info.push(("Number of Devices".to_string(), num_devices.to_string()));
    }
    info
}

/// Returns human-readable memory array location string.
fn memory_array_location_str(code: u8) -> String {
    match code {
        0x01 => "Other".to_string(),
        0x02 => "Unknown".to_string(),
        0x03 => "System board or motherboard".to_string(),
        0x04 => "ISA add-on card".to_string(),
        0x05 => "EISA add-on card".to_string(),
        0x06 => "PCI add-on card".to_string(),
        0x07 => "MCA add-on card".to_string(),
        0x08 => "PCMCIA add-on card".to_string(),
        0x09 => "Proprietary add-on card".to_string(),
        0x0A => "NuBus".to_string(),
        0xA0 => "PC-98/C20 add-on card".to_string(),
        0xA1 => "PC-98/C24 add-on card".to_string(),
        0xA2 => "PC-98/E add-on card".to_string(),
        0xA3 => "PC-98/Local bus add-on card".to_string(),
        0xA4 => "CXL add-on card".to_string(),
        _ => format!("Unknown (0x{:02X})", code),
    }
}

/// Returns human-readable memory array use string.
fn memory_array_use_str(code: u8) -> String {
    match code {
        0x01 => "Other".to_string(),
        0x02 => "Unknown".to_string(),
        0x03 => "System memory".to_string(),
        0x04 => "Video memory".to_string(),
        0x05 => "Flash memory".to_string(),
        0x06 => "Non-volatile RAM".to_string(),
        0x07 => "Cache memory".to_string(),
        _ => format!("Unknown (0x{:02X})", code),
    }
}

/// Returns human-readable memory error correction type string.
fn memory_error_correction_str(code: u8) -> String {
    match code {
        0x01 => "Other".to_string(),
        0x02 => "Unknown".to_string(),
        0x03 => "None".to_string(),
        0x04 => "Parity".to_string(),
        0x05 => "Single-bit ECC".to_string(),
        0x06 => "Multi-bit ECC".to_string(),
        0x07 => "CRC".to_string(),
        _ => format!("Unknown (0x{:02X})", code),
    }
}

/// Parser for SMBIOS Type 19: Memory Array Mapped Address.
fn parse_type_19(data: &[u8], offset: usize, _strings: &[String]) -> Vec<(String, String)> {
    let mut info = Vec::new();
    if offset + 0x0F <= data.len() {
        let start_addr = LittleEndian::read_u32(&data[offset + 0x04..offset + 0x08]);
        let end_addr = LittleEndian::read_u32(&data[offset + 0x08..offset + 0x0C]);
        let array_handle = LittleEndian::read_u16(&data[offset + 0x0C..offset + 0x0E]);
        let partition_width = data[offset + 0x0E];

        if start_addr == 0xFFFFFFFF && end_addr == 0xFFFFFFFF {
            if offset + 0x1F <= data.len() {
                let ext_start = LittleEndian::read_u64(&data[offset + 0x0F..offset + 0x17]);
                let ext_end = LittleEndian::read_u64(&data[offset + 0x17..offset + 0x1F]);
                info.push((
                    "Starting Address".to_string(),
                    format!("0x{:016X}", ext_start),
                ));
                info.push(("Ending Address".to_string(), format!("0x{:016X}", ext_end)));
                let size = (ext_end - ext_start + 1) / (1024 * 1024);
                info.push(("Range Size".to_string(), format!("{} MB", size)));
            } else {
                info.push((
                    "Starting Address".to_string(),
                    "Extended (>4GB)".to_string(),
                ));
                info.push(("Ending Address".to_string(), "Extended (>4GB)".to_string()));
            }
        } else {
            info.push((
                "Starting Address".to_string(),
                format!("0x{:08X} ({} KB)", start_addr, start_addr),
            ));
            info.push((
                "Ending Address".to_string(),
                format!("0x{:08X} ({} KB)", end_addr, end_addr),
            ));
            let size = (end_addr - start_addr + 1) / 1024;
            info.push(("Range Size".to_string(), format!("{} MB", size)));
        }

        info.push((
            "Physical Array Handle".to_string(),
            format!("0x{:04X}", array_handle),
        ));
        info.push(("Partition Width".to_string(), partition_width.to_string()));
    }
    info
}

/// Parser for SMBIOS Type 127: End-of-Table.
fn parse_type_127(_data: &[u8], _offset: usize, _strings: &[String]) -> Vec<(String, String)> {
    vec![(
        "Description".to_string(),
        "End of SMBIOS structure table".to_string(),
    )]
}
