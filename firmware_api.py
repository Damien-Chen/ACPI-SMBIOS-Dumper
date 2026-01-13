import ctypes
from ctypes import wintypes
import sys
import struct

# Load kernel32 with use_last_error=True to capture error codes reliably
kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)

# Define types
DWORD = ctypes.c_ulong
LPVOID = ctypes.c_void_p
PVOID = ctypes.c_void_p

# EnumSystemFirmwareTables
# UINT EnumSystemFirmwareTables(
#   DWORD FirmwareTableProviderSignature,
#   PVOID pFirmwareTableEnumBuffer,
#   DWORD BufferSize
# );
kernel32.EnumSystemFirmwareTables.argtypes = [DWORD, PVOID, DWORD]
kernel32.EnumSystemFirmwareTables.restype = ctypes.c_uint

# GetSystemFirmwareTable
# UINT GetSystemFirmwareTable(
#   DWORD FirmwareTableProviderSignature,
#   DWORD FirmwareTableID,
#   PVOID pFirmwareTableBuffer,
#   DWORD BufferSize
# );
kernel32.GetSystemFirmwareTable.argtypes = [DWORD, DWORD, PVOID, DWORD]
kernel32.GetSystemFirmwareTable.restype = ctypes.c_uint

# Constants
ERROR_INSUFFICIENT_BUFFER = 122
ERROR_ACCESS_DENIED = 5
ERROR_NO_MORE_ITEMS = 259
ERROR_INVALID_FUNCTION = 1

VERBOSE = True

def log(msg):
    if VERBOSE:
        print(f"[DEBUG] {msg}")

def get_firmware_provider_signature(signature_str):
    # 'ACPI' should be 0x41435049 (Big Endian representation of the string)
    # Previous attempts with Little Endian (<I) resulted in 0x49504341 which caused ERROR_INVALID_FUNCTION
    if len(signature_str) != 4:
        raise ValueError("Signature must be 4 characters.")
    
    # Use Big Endian (>I) to result in 0x41435049 for 'ACPI'
    val = struct.unpack('>I', signature_str.encode('ascii'))[0]
    return val

def enum_system_firmware_tables(provider_signature):
    sig_int = get_firmware_provider_signature(provider_signature)
    
    ctypes.set_last_error(0)
    log(f"EnumSystemFirmwareTables: Calling with sig={provider_signature} (0x{sig_int:X})")
    
    size = kernel32.EnumSystemFirmwareTables(sig_int, None, 0)
    err = ctypes.get_last_error()
    log(f"EnumSystemFirmwareTables: Size={size}, Err={err}")
    
    if size == 0:
        if err != 0:
             print(f"[ERROR] EnumSystemFirmwareTables failed. Code: {err} ({ctypes.FormatError(err)})")
        return []

    buffer = (ctypes.c_char * size)()
    ctypes.set_last_error(0)
    ret = kernel32.EnumSystemFirmwareTables(sig_int, buffer, size)
    err = ctypes.get_last_error()
    
    if ret == 0:
        print(f"[ERROR] EnumSystemFirmwareTables (2nd call) failed. Code: {err} ({ctypes.FormatError(err)})")
        return []

    # Parse
    count = ret // 4
    log(f"EnumSystemFirmwareTables: Got {ret} bytes ({count} entries)")
    
    tables = []
    for i in range(count):
        chunk = buffer[i*4 : (i+1)*4]
        try:
            name = chunk.decode('ascii')
        except:
            name = chunk.hex()
        tables.append(name)
        
    return tables

def get_system_firmware_table(provider_signature, table_id):
    sig_int = get_firmware_provider_signature(provider_signature)
    
    if isinstance(table_id, str):
        table_id_int = struct.unpack('<I', table_id.encode('ascii'))[0]
        desc = f"'{table_id}'"
    else:
        table_id_int = table_id
        desc = f"{table_id}"

    ctypes.set_last_error(0)
    # log(f"GetSystemFirmwareTable: Provider={provider_signature}, Table={desc}")
    
    size = kernel32.GetSystemFirmwareTable(sig_int, table_id_int, None, 0)
    err = ctypes.get_last_error()
    
    if size == 0:
        # If size is 0 and err is not 0, it's an error. 
        # CAREFUL: Some implementations might return 0 size for empty tables with err=0.
        if err != 0:
            print(f"[ERROR] GetSystemFirmwareTable({provider_signature}, {desc}) failed. Code: {err} ({ctypes.FormatError(err)})")
        return b""

    buffer = (ctypes.c_char * size)()
    ctypes.set_last_error(0)
    ret = kernel32.GetSystemFirmwareTable(sig_int, table_id_int, buffer, size)
    err = ctypes.get_last_error()
    
    if ret == 0:
        print(f"[ERROR] GetSystemFirmwareTable (2nd call) failed. Code: {err} ({ctypes.FormatError(err)})")
        return b""
        
    return bytes(buffer)

def get_smbios_data():
    return get_system_firmware_table('RSMB', 0)

def list_acpi_tables():
    return enum_system_firmware_tables('ACPI')

def get_acpi_table(signature):
    return get_system_firmware_table('ACPI', signature)

if __name__ == "__main__":
    print("--- Diagnostics ---")
    print(f"Python: {sys.version} ({sys.api_version})")
    print(f"Is 64-bit: {sys.maxsize > 2**32}")
    
    try:
        admin = ctypes.windll.shell32.IsUserAnAdmin()
        print(f"Is Admin: {admin}")
    except:
        print("Is Admin: Unknown")

    print("\n--- TEST ACPI ---")
    tables = list_acpi_tables()
    print(f"Tables: {tables}")
    
    print("\n--- TEST SMBIOS ---")
    smbios = get_smbios_data()
    print(f"SMBIOS Data: {len(smbios) if smbios else 0} bytes")
