import struct
from collections import namedtuple

# Named tuples for structured data
AcpiTableHeader = namedtuple('AcpiTableHeader', [
    'signature', 'length', 'revision', 'checksum', 
    'oem_id', 'oem_table_id', 'oem_revision', 
    'creator_id', 'creator_revision'
])

SmbiosStructureHeader = namedtuple('SmbiosStructureHeader', [
    'type', 'length', 'handle'
])

RawSMBIOSData = namedtuple('RawSMBIOSData', [
    'used_20_calling_method', 'major_version', 'minor_version',
    'dmi_revision', 'length'
])

def parse_raw_smbios_data_header(data):
    """
    Parses the Windows RawSMBIOSData header.
    Returns (header_obj, data_offset).
    If header seems invalid or data too short, returns (None, 0).
    """
    if len(data) < 8:
        return None, 0
    
    # struct RawSMBIOSData {
    #   BYTE  Used20CallingMethod;
    #   BYTE  SMBIOSMajorVersion;
    #   BYTE  SMBIOSMinorVersion;
    #   BYTE  DmiRevision;
    #   DWORD Length;
    #   BYTE  SMBIOSTableData[];
    # };
    try:
        u20, maj, min_, dmi, length = struct.unpack('<BBBBI', data[:8])
        header = RawSMBIOSData(u20, maj, min_, dmi, length)
        # The actual SMBIOS data follows immediately
        return header, 8
    except:
        return None, 0

def parse_acpi_header(data):
    """
    Parses the standard 36-byte ACPI table header.
    """
    if len(data) < 36:
        raise ValueError("Data too short for ACPI header")

    # Unpack
    fmt = "<4sIBB6s8sI4sI"
    parts = struct.unpack(fmt, data[:36])
    
    # helper to clean byte strings
    def clean_str(b):
        try:
            return b.decode('ascii', errors='ignore').strip('\x00')
        except:
            return b.hex()

    return AcpiTableHeader(
        signature=clean_str(parts[0]),
        length=parts[1],
        revision=parts[2],
        checksum=parts[3],
        oem_id=clean_str(parts[4]),
        oem_table_id=clean_str(parts[5]),
        oem_revision=parts[6],
        creator_id=clean_str(parts[7]),
        creator_revision=parts[8]
    )

def parse_smbios_structure(data, offset):
    """
    Parses an SMBIOS structure header at the given offset.
    Returns (header, next_offset) or (None, None) if end.
    """
    if offset + 4 > len(data):
        return None, None
    
    type_, length, handle = struct.unpack('<BBH', data[offset:offset+4])
    
    # Valid SMBIOS structure must have length >= 4
    if length < 4:
        return None, None
        
    header = SmbiosStructureHeader(type=type_, length=length, handle=handle)
    
    # Calculate end of formatted section
    formatted_end = offset + length
    
    # Find double null terminator after formatted_end
    current = formatted_end
    while current + 1 < len(data):
        if data[current] == 0 and data[current+1] == 0:
            return header, current + 2
        current += 1
    
    return header, len(data)

def get_smbios_strings(data, offset, length):
    """
    Extracts strings from the unformatted section.
    """
    strings = []
    str_start = offset + length
    current_idx = str_start
    while current_idx < len(data):
        try:
            null_idx = data.index(b'\x00', current_idx)
        except ValueError:
            break
            
        if null_idx == current_idx:
            break
            
        s_bytes = data[current_idx:null_idx]
        try:
            s = s_bytes.decode('utf-8', errors='ignore')
        except:
            s = s_bytes.hex()
        strings.append(s)
        
        current_idx = null_idx + 1
        if current_idx < len(data) and data[current_idx] == 0:
             break
             
    return strings

def get_string_by_index(strings, index):
    """Returns string at index (1-based) or 'Not Specified'."""
    if index == 0:
        return "None" # 0 means no string
    if 0 < index <= len(strings):
        return strings[index-1]
    return f"<Bad String Index: {index}>"

# --- Specific Parsers ---

def parse_type_0(data, offset, length, strings):
    # BIOS Information
    info = []
    try:
        vendor_idx = data[offset + 0x04]
        ver_idx = data[offset + 0x05]
        date_idx = data[offset + 0x08]
        rom_size_enc = data[offset + 0x09]
        
        info.append(("Vendor", get_string_by_index(strings, vendor_idx)))
        info.append(("Version", get_string_by_index(strings, ver_idx)))
        info.append(("Release Date", get_string_by_index(strings, date_idx)))
        
        # ROM Size calculation
        if rom_size_enc == 0xFF:
            # Look at extended
            pass # TODO support extended
        else:
            size_kb = (rom_size_enc + 1) * 64
            info.append(("ROM Size", f"{size_kb} KB"))

    except Exception as e:
        info.append(("Parse Error", str(e)))
    return info

def parse_type_1(data, offset, length, strings):
    # System Information
    info = []
    try:
        man_idx = data[offset + 0x04]
        prod_idx = data[offset + 0x05]
        ver_idx = data[offset + 0x06]
        ser_idx = data[offset + 0x07]
        
        info.append(("Manufacturer", get_string_by_index(strings, man_idx)))
        info.append(("Product Name", get_string_by_index(strings, prod_idx)))
        info.append(("Version", get_string_by_index(strings, ver_idx)))
        info.append(("Serial Number", get_string_by_index(strings, ser_idx)))
        
        # UUID
        if length > 0x08:
            uuid_bytes = data[offset + 0x08 : offset + 0x18]
            # SMBIOS UUID encoding is weird (first 3 fields match LE/BE depending on version)
            # Just hex dumping is usually safer for now or standard UUID display
            import uuid
            try:
                # Assuming standard network order for simplicity or just raw hex
                # Windows might display it differently
                u = uuid.UUID(bytes_le=uuid_bytes) # Often little endian in first parts
                info.append(("UUID", str(u).upper()))
            except:
                info.append(("UUID", uuid_bytes.hex()))

    except Exception as e:
        info.append(("Parse Error", str(e)))
    return info

def parse_type_2(data, offset, length, strings):
    # Baseboard
    info = []
    try:
        man_idx = data[offset + 0x04]
        prod_idx = data[offset + 0x05]
        ver_idx = data[offset + 0x06]
        ser_idx = data[offset + 0x07]
        asset_idx = data[offset + 0x08] if length > 0x08 else 0

        info.append(("Manufacturer", get_string_by_index(strings, man_idx)))
        info.append(("Product Name", get_string_by_index(strings, prod_idx)))
        info.append(("Version", get_string_by_index(strings, ver_idx)))
        info.append(("Serial Number", get_string_by_index(strings, ser_idx)))
        info.append(("Asset Tag", get_string_by_index(strings, asset_idx)))

    except Exception as e:
         info.append(("Parse Error", str(e)))
    return info

def parse_type_3(data, offset, length, strings):
    # Chassis
    info = []
    try:
        man_idx = data[offset + 0x04]
        type_code = data[offset + 0x05]
        ver_idx = data[offset + 0x06]
        ser_idx = data[offset + 0x07]
        
        info.append(("Manufacturer", get_string_by_index(strings, man_idx)))
        info.append(("Type", f"0x{type_code:02X}")) # Could decode enum
        info.append(("Version", get_string_by_index(strings, ver_idx)))
        info.append(("Serial Number", get_string_by_index(strings, ser_idx)))
        
    except Exception as e:
         info.append(("Parse Error", str(e)))
    return info

def parse_type_4(data, offset, length, strings):
    # Processor
    info = []
    try:
        sock_idx = data[offset + 0x04]
        type_enum = data[offset + 0x05]
        man_idx = data[offset + 0x07]
        ver_idx = data[offset + 0x10]
        
        info.append(("Socket Designator", get_string_by_index(strings, sock_idx)))
        info.append(("Processor Type", f"0x{type_enum:02X}"))
        info.append(("Manufacturer", get_string_by_index(strings, man_idx)))
        info.append(("Version", get_string_by_index(strings, ver_idx)))
        
        # Core Count, etc.
        if length >= 0x23:
            core_count = data[offset + 0x23]
            thread_count = data[offset + 0x25]
            info.append(("Core Count", str(core_count)))
            info.append(("Thread Count", str(thread_count)))

    except Exception as e:
         info.append(("Parse Error", str(e)))
    return info

def parse_type_17(data, offset, length, strings):
    # Memory Device
    info = []
    try:
        # device_loc_idx = data[offset + 0x10]
        # bank_loc_idx   = data[offset + 0x11]
        
        total_width = struct.unpack('<H', data[offset + 0x08 : offset + 0x0A])[0]
        data_width  = struct.unpack('<H', data[offset + 0x0A : offset + 0x0C])[0]
        size        = struct.unpack('<H', data[offset + 0x0C : offset + 0x0E])[0]
        speed       = struct.unpack('<H', data[offset + 0x15 : offset + 0x17])[0]
        
        man_idx     = data[offset + 0x17]
        ser_idx     = data[offset + 0x18]
        asset_idx   = data[offset + 0x19]
        part_idx    = data[offset + 0x1A]
        dev_idx     = data[offset + 0x10]
        bank_idx    = data[offset + 0x11]

        info.append(("Device Locator", get_string_by_index(strings, dev_idx)))
        info.append(("Bank Locator", get_string_by_index(strings, bank_idx)))
        
        # Size
        if size == 0xFFFF:
            info.append(("Size", "Unknown / Extended"))
        elif size == 0:
            info.append(("Size", "No Module Installed"))
        else:
            if size & 0x8000:
                s_val = size & 0x7FFF
                info.append(("Size", f"{s_val} KB"))
            else:
                 info.append(("Size", f"{size} MB"))

        info.append(("Speed", f"{speed} MT/s" if speed != 0 else "Unknown"))
        info.append(("Manufacturer", get_string_by_index(strings, man_idx)))
        info.append(("Serial Number", get_string_by_index(strings, ser_idx)))
        info.append(("Asset Tag", get_string_by_index(strings, asset_idx)))
        info.append(("Part Number", get_string_by_index(strings, part_idx)))
        
        info.append(("Total Width", f"{total_width} bits"))
        info.append(("Data Width", f"{data_width} bits"))

    except Exception as e:
         info.append(("Parse Error", str(e)))
    return info

def get_parsed_smbios_info(type_id, data, offset, length, strings):
    """
    Dispatcher for specific parsers.
    Returns list of (key, value) tuples or None if no specific parser.
    """
    parsers = {
        0: parse_type_0,
        1: parse_type_1,
        2: parse_type_2,
        3: parse_type_3,
        4: parse_type_4,
        17: parse_type_17
    }
    
    if type_id in parsers:
        return parsers[type_id](data, offset, length, strings)
    return None

if __name__ == "__main__":
    pass
