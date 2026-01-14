use windows::Win32::System::SystemInformation::{EnumSystemFirmwareTables, GetSystemFirmwareTable, FIRMWARE_TABLE_PROVIDER};
use windows::Win32::Foundation::GetLastError;

pub const SIG_ACPI: u32 = u32::from_be_bytes(*b"ACPI"); // 0x41435049
pub const SIG_RSMB: u32 = u32::from_be_bytes(*b"RSMB"); // 0x52534D42

pub fn enum_system_firmware_tables(provider: u32) -> Result<Vec<String>, String> {
    let provider = FIRMWARE_TABLE_PROVIDER(provider);
    unsafe {
        // First call to get size
        let size = EnumSystemFirmwareTables(provider, None);
        if size == 0 {
            let err = GetLastError();
            return Err(format!("EnumSystemFirmwareTables failed. Code: {:?}", err));
        }

        let mut buffer = vec![0u8; size as usize];
        let ret = EnumSystemFirmwareTables(provider, Some(&mut buffer));
        if ret == 0 {
             let err = GetLastError();
             return Err(format!("EnumSystemFirmwareTables (2nd call) failed. Code: {:?}", err));
        }
        
        // Parse buffer (array of 4-byte signatures usually, or strings? MSDN says:
        // "The buffer contains an array of 4-byte headers..." for ACPI it's table signatures.
        // For ACPI, it is an array of 4-char signatures.
        
        let count = (ret as usize) / 4;
        let mut tables = Vec::new();
        
        for i in 0..count {
            let start = i * 4;
            let end = start + 4;
            let chunk = &buffer[start..end];
            // Try to decode as ASCII, else hex
            match std::str::from_utf8(chunk) {
                Ok(s) => tables.push(s.to_string()),
                Err(_) => tables.push(hex::encode(chunk).to_uppercase()),
            }
        }
        
        Ok(tables)
    }
}

pub fn get_system_firmware_table(provider: u32, table_id: &str) -> Result<Vec<u8>, String> {
    let provider_u32 = provider; // keep raw for logic if needed, but we need FIRMWARE_TABLE_PROVIDER for call
    let provider_type = FIRMWARE_TABLE_PROVIDER(provider);

    // For ACPI, table_id is the signature (e.g. "FACP").
    // We need to convert string to u32 (Little Endian for GetSystemFirmwareTable? 
    // MSDN: "The identifier of the firmware table to be retrieved. This parameter is interpreted slightly differently depending on the firmware table provider."
    // For 'ACPI', it's the signature.
    // For 'RSMB', it's 0 (Raw SMBIOS).
    
    let id_int = if provider_u32 == SIG_RSMB {
        0
    } else {
        // Assume ACPI or generic 4-char ID. 
        // Python code: struct.unpack('<I', table_id.encode('ascii'))[0] which is Little Endian.
        if table_id.len() != 4 {
             // Special case for RSMB if user passed "0" or empty, but generally we expect 4 chars for ACPI
             if table_id == "0" { 0 } else {
                 return Err("Table ID must be 4 characters for ACPI".into());
             }
        } else {
             let bytes = table_id.as_bytes();
             u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
    };

    unsafe {
        let size = GetSystemFirmwareTable(provider_type, id_int, None);
        if size == 0 {
             // Could be empty or error.
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
             return Err(format!("GetSystemFirmwareTable (2nd call) failed. Code: {:?}", err));
        }

        Ok(buffer)
    }
}

pub fn get_smbios_data() -> Result<Vec<u8>, String> {
    get_system_firmware_table(SIG_RSMB, "0")
}
