use std::ffi::CStr;
use windows::Win32::Foundation::GetLastError;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExA, RegEnumValueA, RegOpenKeyExA, RegQueryValueExA, HKEY,
    HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY,
};
use windows::Win32::System::SystemInformation::{
    EnumSystemFirmwareTables, GetSystemFirmwareTable, FIRMWARE_TABLE_PROVIDER,
};

/// ACPI firmware table provider signature ('ACPI').
pub const SIG_ACPI: u32 = u32::from_be_bytes(*b"ACPI"); // 0x41435049
/// SMBIOS firmware table provider signature ('RSMB').
pub const SIG_RSMB: u32 = u32::from_be_bytes(*b"RSMB"); // 0x52534D42

/// Information about an ACPI table discovered in the system.
///
/// This structure holds metadata for identifying and retrieving the actual table data.
#[derive(Debug, Clone, PartialEq)]
pub struct AcpiTableInfo {
    /// The real 4-byte signature read from the binary data (e.g., "SSDT").
    pub signature: String,
    /// The signature as it appears in the registry key (e.g., "SSD1").
    pub registry_sig: String,
    /// The OEM ID from the ACPI header.
    pub oem_id: String,
    /// The Table ID from the ACPI header.
    pub table_id: String,
    /// The table revision.
    pub revision: u32,
    /// Optional full registry path to the table data.
    pub registry_path: Option<String>,
    /// Optional physical address of the table (if known).
    pub physical_address: Option<u64>,
}

/// Helper to read the real signature (first 4 bytes) from table binary data at a given registry path.
///
/// # Arguments
/// * `path` - The registry path string where the table data is stored.
///
/// # Returns
/// An `Option<String>` containing the 4-character signature if valid UTF-8, otherwise `None`.
fn read_real_signature(path: &str) -> Option<String> {
    if let Ok(data) = get_acpi_table_by_path(path) {
        if data.len() >= 4 {
            let sig_bytes = &data[0..4];
            if let Ok(s) = std::str::from_utf8(sig_bytes) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// Enumerates ACPI tables by traversing the Windows Registry (`HKLM\HARDWARE\ACPI`).
///
/// This method is useful for finding duplicate tables (like multiple SSDTs) that the standard
/// `EnumSystemFirmwareTables` API might not return as distinct entries.
///
/// # Returns
/// A `Result` containing a vector of `AcpiTableInfo` on success, or an error string on failure.
pub fn enum_acpi_tables_registry() -> Result<Vec<AcpiTableInfo>, String> {
    let mut tables = Vec::new();
    let root_path = "HARDWARE\\ACPI\0";

    unsafe {
        let mut h_root = HKEY::default();
        if RegOpenKeyExA(
            HKEY_LOCAL_MACHINE,
            windows::core::PCSTR(root_path.as_ptr()),
            0,
            KEY_READ,
            &mut h_root,
        )
        .is_err()
        {
            return Err(
                "Failed to open HKLM\\HARDWARE\\ACPI. Verify the tool is running as Administrator."
                    .into(),
            );
        }

        let mut sig_idx = 0;
        loop {
            let mut sig_name = [0u8; 256];
            let mut sig_name_len = sig_name.len() as u32;
            if RegEnumKeyExA(
                h_root,
                sig_idx,
                windows::core::PSTR(sig_name.as_mut_ptr()),
                &mut sig_name_len,
                None,
                windows::core::PSTR(std::ptr::null_mut()),
                None,
                None,
            )
            .is_err()
            {
                break;
            }
            let reg_sig_str = CStr::from_ptr(sig_name.as_ptr() as *const i8)
                .to_string_lossy()
                .into_owned();

            // Open Signature Key
            let mut h_sig = HKEY::default();
            if RegOpenKeyExA(
                h_root,
                windows::core::PCSTR(sig_name.as_ptr()),
                0,
                KEY_READ,
                &mut h_sig,
            )
            .is_ok()
            {
                let mut oem_idx = 0;
                loop {
                    let mut oem_name = [0u8; 256];
                    let mut oem_name_len = oem_name.len() as u32;
                    if RegEnumKeyExA(
                        h_sig,
                        oem_idx,
                        windows::core::PSTR(oem_name.as_mut_ptr()),
                        &mut oem_name_len,
                        None,
                        windows::core::PSTR(std::ptr::null_mut()),
                        None,
                        None,
                    )
                    .is_err()
                    {
                        break;
                    }
                    let oem_str = CStr::from_ptr(oem_name.as_ptr() as *const i8)
                        .to_string_lossy()
                        .into_owned();

                    // Open OEM Key
                    let mut h_oem = HKEY::default();
                    if RegOpenKeyExA(
                        h_sig,
                        windows::core::PCSTR(oem_name.as_ptr()),
                        0,
                        KEY_READ,
                        &mut h_oem,
                    )
                    .is_ok()
                    {
                        let mut tab_idx = 0;
                        loop {
                            let mut tab_name = [0u8; 256];
                            let mut tab_name_len = tab_name.len() as u32;
                            if RegEnumKeyExA(
                                h_oem,
                                tab_idx,
                                windows::core::PSTR(tab_name.as_mut_ptr()),
                                &mut tab_name_len,
                                None,
                                windows::core::PSTR(std::ptr::null_mut()),
                                None,
                                None,
                            )
                            .is_err()
                            {
                                break;
                            }
                            let tab_str = CStr::from_ptr(tab_name.as_ptr() as *const i8)
                                .to_string_lossy()
                                .into_owned();

                            // Open Table ID Key
                            let mut h_tab = HKEY::default();
                            if RegOpenKeyExA(
                                h_oem,
                                windows::core::PCSTR(tab_name.as_ptr()),
                                0,
                                KEY_READ,
                                &mut h_tab,
                            )
                            .is_ok()
                            {
                                let mut rev_idx = 0;
                                loop {
                                    let mut rev_name = [0u8; 256];
                                    let mut rev_name_len = rev_name.len() as u32;
                                    if RegEnumKeyExA(
                                        h_tab,
                                        rev_idx,
                                        windows::core::PSTR(rev_name.as_mut_ptr()),
                                        &mut rev_name_len,
                                        None,
                                        windows::core::PSTR(std::ptr::null_mut()),
                                        None,
                                        None,
                                    )
                                    .is_err()
                                    {
                                        break;
                                    }
                                    let rev_str = CStr::from_ptr(rev_name.as_ptr() as *const i8)
                                        .to_string_lossy()
                                        .into_owned();
                                    let rev_val = u32::from_str_radix(&rev_str, 16).unwrap_or(0);

                                    let full_path = format!(
                                        "HARDWARE\\ACPI\\{}\\{}\\{}\\{}",
                                        reg_sig_str, oem_str, tab_str, rev_str
                                    );

                                    // Read real signature from binary data
                                    let real_sig = read_real_signature(&full_path)
                                        .unwrap_or_else(|| reg_sig_str.clone());

                                    // Try to find physical address (not always in registry, but sometimes in subkeys)
                                    let physical_address = None;

                                    tables.push(AcpiTableInfo {
                                        signature: real_sig,
                                        registry_sig: reg_sig_str.clone(),
                                        oem_id: oem_str.clone(),
                                        table_id: tab_str.clone(),
                                        revision: rev_val,
                                        registry_path: Some(full_path),
                                        physical_address,
                                    });

                                    rev_idx += 1;
                                }
                                let _ = RegCloseKey(h_tab);
                            }
                            tab_idx += 1;
                        }
                        let _ = RegCloseKey(h_oem);
                    }
                    oem_idx += 1;
                }
                let _ = RegCloseKey(h_sig);
            }
            sig_idx += 1;
        }
        let _ = RegCloseKey(h_root);
    }

    Ok(tables)
}

/// Loads ACPI tables by combining Registry enumeration and the Windows System Firmware API.
///
/// This provides a comprehensive list by prioritizing Registry results (for duplicates)
/// and supplementing them with API results (for system-managed tables like UEFI which might not be in the registry).
///
/// # Returns
/// A vector of `AcpiTableInfo`.
pub fn load_acpi_tables_combined() -> Vec<AcpiTableInfo> {
    let mut combined = Vec::new();

    // 1. Load from Registry (Priority for duplicates)
    if let Ok(reg_tables) = enum_acpi_tables_registry() {
        combined.extend(reg_tables);
    }

    // 2. Load from API (Fallback for missing tables like UEFI)
    if let Ok(api_sigs) = enum_system_firmware_tables(SIG_ACPI) {
        for sig in api_sigs {
            // If already present in registry collection, skip
            if combined.iter().any(|t| t.signature == sig) {
                continue;
            }

            // Try to fetch table data to get header info
            if let Ok(data) = get_system_firmware_table(SIG_ACPI, &sig) {
                if data.len() >= 36 {
                    let oem_id = String::from_utf8_lossy(&data[10..16]).trim().to_string();
                    let table_id = String::from_utf8_lossy(&data[16..24]).trim().to_string();
                    let revision = data[8] as u32;

                    combined.push(AcpiTableInfo {
                        signature: sig.clone(),
                        registry_sig: sig.clone(),
                        oem_id,
                        table_id,
                        revision,
                        registry_path: None,
                        physical_address: None, // API doesn't give physical address either
                    });
                }
            }
        }
    }

    combined
}

/// Retrieves the raw binary content of an ACPI table from the Windows Registry using its full path.
///
/// # Arguments
/// * `path` - Full path to the registry key containing the table binary.
///
/// # Returns
/// A `Result` containing the binary data as a `Vec<u8>` on success, or an error string on failure.
pub fn get_acpi_table_by_path(path: &str) -> Result<Vec<u8>, String> {
    unsafe {
        let mut h_key = HKEY::default();
        let path_null = format!("{}\0", path);
        if RegOpenKeyExA(
            HKEY_LOCAL_MACHINE,
            windows::core::PCSTR(path_null.as_ptr()),
            0,
            KEY_READ,
            &mut h_key,
        )
        .is_err()
        {
            return Err(format!("Key open fail: {}", path));
        }

        // Try value name "0" first (common location for ACPI binary data)
        let mut size = 0u32;
        if RegQueryValueExA(
            h_key,
            windows::core::PCSTR(c"0".as_ptr() as *const u8),
            None,
            None,
            None,
            Some(&mut size),
        )
        .is_ok()
        {
            let mut buffer = vec![0u8; size as usize];
            if RegQueryValueExA(
                h_key,
                windows::core::PCSTR(c"0".as_ptr() as *const u8),
                None,
                None,
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            )
            .is_ok()
            {
                let _ = RegCloseKey(h_key);
                return Ok(buffer);
            }
        }

        // Fallback: enumerate all values in the key and find the first binary one
        let mut val_idx = 0;
        loop {
            let mut val_name = [0u8; 256];
            let mut val_name_len = val_name.len() as u32;
            let mut val_type = 0u32;
            let mut val_size = 0u32;

            if RegEnumValueA(
                h_key,
                val_idx,
                windows::core::PSTR(val_name.as_mut_ptr()),
                &mut val_name_len,
                None,
                Some(&mut val_type),
                None,
                Some(&mut val_size),
            )
            .is_err()
            {
                break;
            }

            if val_type == REG_BINARY.0 {
                let mut buffer = vec![0u8; val_size as usize];
                if RegQueryValueExA(
                    h_key,
                    windows::core::PCSTR(val_name.as_ptr()),
                    None,
                    None,
                    Some(buffer.as_mut_ptr()),
                    Some(&mut val_size),
                )
                .is_ok()
                {
                    let _ = RegCloseKey(h_key);
                    return Ok(buffer);
                }
            }
            val_idx += 1;
        }

        let _ = RegCloseKey(h_key);
        Err(format!("No binary value found in registry key: {}", path))
    }
}

/// Enumerates available ACPI tables using the `EnumSystemFirmwareTables` Windows API.
///
/// In addition to the API enumeration, this function proactively checks for common
/// tables that are often not returned by the enumeration but are still accessible
/// (e.g., DSDT, XSDT, RSDP).
///
/// # Arguments
/// * `provider` - The firmware table provider signature (e.g., `SIG_ACPI`).
///
/// # Returns
/// A `Result` containing a vector of table signature strings on success.
pub fn enum_system_firmware_tables(provider: u32) -> Result<Vec<String>, String> {
    let provider = FIRMWARE_TABLE_PROVIDER(provider);
    unsafe {
        let size = EnumSystemFirmwareTables(provider, None);
        if size == 0 {
            let err = GetLastError();
            return Err(format!("EnumSystemFirmwareTables failed. Code: {:?}", err));
        }

        let mut buffer = vec![0u8; size as usize];
        let ret = EnumSystemFirmwareTables(provider, Some(&mut buffer));
        if ret == 0 {
            let err = GetLastError();
            return Err(format!(
                "EnumSystemFirmwareTables (2nd call) failed. Code: {:?}",
                err
            ));
        }

        let count = (ret as usize) / 4;
        let mut tables = Vec::new();
        for i in 0..count {
            let start = i * 4;
            let end = start + 4;
            let chunk = &buffer[start..end];
            match std::str::from_utf8(chunk) {
                Ok(s) => tables.push(s.to_string()),
                Err(_) => tables.push(hex::encode(chunk).to_uppercase()),
            }
        }

        // Proactively check for "hidden" or standard ACPI tables
        let hidden_tables = ["DSDT", "RSDT", "XSDT", "RSDP", "UEFI"];
        for &sig in &hidden_tables {
            if !tables.contains(&sig.to_string()) {
                let id_int = u32::from_le_bytes(sig.as_bytes().try_into().unwrap());
                let size = GetSystemFirmwareTable(provider, id_int, None);
                if size > 0 {
                    tables.push(sig.to_string());
                }
            }
        }

        Ok(tables)
    }
}

/// Retrieves raw binary data for a specific ACPI or SMBIOS table using the `GetSystemFirmwareTable` API.
///
/// # Arguments
/// * `provider` - The firmware table provider signature (`SIG_ACPI` or `SIG_RSMB`).
/// * `table_id` - The 4-character signature for ACPI, or "0" for SMBIOS.
///
/// # Returns
/// A `Result` containing the binary data as `Vec<u8>`.
pub fn get_system_firmware_table(provider: u32, table_id: &str) -> Result<Vec<u8>, String> {
    let provider_u32 = provider;
    let provider_type = FIRMWARE_TABLE_PROVIDER(provider);

    let id_int = if provider_u32 == SIG_RSMB {
        0
    } else if table_id.len() != 4 {
        if table_id == "0" {
            0
        } else {
            return Err("Table ID must be 4 characters for ACPI".into());
        }
    } else {
        let bytes = table_id.as_bytes();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    };

    unsafe {
        let size = GetSystemFirmwareTable(provider_type, id_int, None);
        if size == 0 {
            let err = GetLastError();
            if err.is_err() {
                return Err(format!("GetSystemFirmwareTable failed. Code: {:?}", err));
            }
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; size as usize];
        let ret = GetSystemFirmwareTable(provider_type, id_int, Some(&mut buffer));
        if ret == 0 {
            let err = GetLastError();
            return Err(format!(
                "GetSystemFirmwareTable (2nd call) failed. Code: {:?}",
                err
            ));
        }

        Ok(buffer)
    }
}

/// Fetches the raw SMBIOS data from the system.
///
/// # Returns
/// A `Result` containing the raw SMBIOS binary data.
pub fn get_smbios_data() -> Result<Vec<u8>, String> {
    get_system_firmware_table(SIG_RSMB, "0")
}
