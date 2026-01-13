import argparse
import sys
import ctypes
from firmware_api import list_acpi_tables, get_acpi_table, get_smbios_data
from parsers import parse_acpi_header, parse_smbios_structure, get_smbios_strings, get_parsed_smbios_info, parse_raw_smbios_data_header

# Try importing rich
try:
    from rich.console import Console
    from rich.table import Table
    from rich.text import Text
    from rich.console import Group
    console = Console()
    HAS_RICH = True
except ImportError:
    HAS_RICH = False
    print("Warning: 'rich' library not found. Output will be plain text.")

def is_admin():
    try:
        return ctypes.windll.shell32.IsUserAnAdmin()
    except:
        return False

def hex_dump(data, length=16):
    """Generates a hex dump of data."""
    if HAS_RICH:
        # We can produce a nice rich output or just return text lines
        pass
        
    lines = []
    for i in range(0, len(data), length):
        chunk = data[i:i+length]
        hex_part = ' '.join(f"{b:02X}" for b in chunk)
        ascii_part = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
        lines.append(f"{i:04X}  {hex_part:<{length*3}}  {ascii_part}")
    return "\n".join(lines)

def print_hex_rich(title, data):
    if not HAS_RICH:
        print(f"--- {title} ---")
        print(hex_dump(data))
        return

    table = Table(title=title, show_header=True, header_style="bold magenta")
    table.add_column("Offset", style="dim", width=8)
    table.add_column("Hex", width=48, style="cyan")
    table.add_column("ASCII", width=16)

    length = 16
    for i in range(0, len(data), length):
        chunk = data[i:i+length]
        hex_part = ' '.join(f"{b:02X}" for b in chunk)
        ascii_part = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
        table.add_row(f"{i:04X}", hex_part, ascii_part)
    
    console.print(table)

def cmd_list_acpi():
    tables = list_acpi_tables()
    if not tables:
        print("No ACPI tables found or access denied.")
        return

    if HAS_RICH:
        table = Table(title="ACPI Tables", show_header=True)
        table.add_column("Signature", style="green")
        for t in tables:
            table.add_row(t)
        console.print(table)
    else:
        print("ACPI Tables:")
        for t in tables:
            print(f" - {t}")

def cmd_dump_acpi(signature):
    data = get_acpi_table(signature)
    if not data:
        print(f"Could not retrieve table '{signature}'")
        return

    # Parse header
    try:
        header = parse_acpi_header(data)
        print(f"Signature: {header.signature}")
        print(f"Length:    {header.length} bytes")
        print(f"OEM ID:    {header.oem_id}")
        print(f"Table ID:  {header.oem_table_id}")
    except Exception as e:
        print(f"Error parsing header: {e}")

    print_hex_rich(f"ACPI Table: {signature}", data)

def cmd_smbios():
    data = get_smbios_data()
    if not data:
        print("Could not retrieve SMBIOS data.")
        return

    print(f"Total Retrieved Data Size: {len(data)} bytes")
    
    # Check for Windows RawSMBIOSData header
    raw_header, offset = parse_raw_smbios_data_header(data)
    if raw_header:
        print(f"Found Windows SMBIOS Header:")
        print(f"  Version: {raw_header.major_version}.{raw_header.minor_version}")
        print(f"  DMI Revision: {raw_header.dmi_revision}")
        print(f"  Table Length: {raw_header.length} bytes")
        # Start processing after the header
    else:
        # Fallback to 0 if not found (should not happen on Windows API)
        offset = 0

    actual_data_len = len(data) - offset
    print(f"Parsing SMBIOS structures starting at offset {offset} ({actual_data_len} bytes)...")

    current_off = offset
    structure_count = 0
    
    # Safety limit
    while current_off < len(data):
        header, next_offset = parse_smbios_structure(data, current_off)
        if not header:
            # Maybe end of list (double nulls usually mark end of table, or just end of buffer)
            break
            
        structure_count += 1
        
        # Get strings
        strings = get_smbios_strings(data, current_off, header.length)
        
        # Get detailed info
        details = get_parsed_smbios_info(header.type, data, current_off, header.length, strings)

        # Print Header
        print("=" * 60)
        title = f"Type {header.type} (Handle 0x{header.handle:04X})"
        
        type_names = {
            0: "BIOS Information", 1: "System Information", 2: "Baseboard Information",
            3: "Chassis Information", 4: "Processor Information", 17: "Memory Device"
        }
        if header.type in type_names:
            title += f" - {type_names[header.type]}"
            
        print(f"{title:^60}")
        print("=" * 60)

        # Print Details or Fallback
        if details:
            for key, val in details:
                print(f"{key:25}: {val}")
        else:
            print(f"Length: 0x{header.length:X}")
            if strings:
                print("-" * 60)
                for i, s in enumerate(strings):
                    print(f"String {i+1}: {s}")
        
        print("\n")
        
        current_off = next_offset
        
    print(f"Finished. Parsed {structure_count} structures.")

def main():
    parser = argparse.ArgumentParser(description="ACPI & SMBIOS Viewer Tool")
    parser.add_argument("--acpi", action="store_true", help="List ACPI tables")
    parser.add_argument("--table", type=str, help="Dump specific ACPI table by signature")
    parser.add_argument("--smbios", action="store_true", help="Dump SMBIOS structures")
    parser.add_argument("--gui", action="store_true", help="Launch GUI mode")
    
    args = parser.parse_args()

    # Check admin
    if not is_admin():
        msg = "WARNING: Not running as Administrator. Firmware APIs will likely fail."
        if HAS_RICH:
            console.print(f"[bold red]{msg}[/bold red]")
        else:
            print(msg)

    if args.gui:
        # Launch GUI
        try:
            from gui_main import run_gui
            run_gui()
        except ImportError:
            print("GUI module not found or not implemented yet.")
        except Exception as e:
            print(f"Error launching GUI: {e}")
        return

    if args.acpi:
        cmd_list_acpi()
    elif args.table:
        cmd_dump_acpi(args.table)
    elif args.smbios:
        cmd_smbios()
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
