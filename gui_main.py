import sys
from PyQt6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, 
                             QHBoxLayout, QTreeWidget, QTreeWidgetItem, 
                             QTextEdit, QSplitter, QTabWidget, QLabel, QMessageBox)
from PyQt6.QtCore import Qt
from PyQt6.QtGui import QFont, QColor

import firmware_api
import parsers

class HexViewer(QTextEdit):
    def __init__(self):
        super().__init__()
        self.setReadOnly(True)
        self.setFont(QFont("Consolas", 10))
    
    def set_data(self, data):
        if not data:
            self.setText("")
            return
            
        lines = []
        length = 16
        for i in range(0, len(data), length):
            chunk = data[i:i+length]
            hex_part = ' '.join(f"{b:02X}" for b in chunk)
            ascii_part = ''.join(chr(b) if 32 <= b < 127 else '.' for b in chunk)
            lines.append(f"{i:04X}  {hex_part:<{length*3}}  {ascii_part}")
        
        self.setText("\n".join(lines))

class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("BIOS Dump Tool - ACPI & SMBIOS Viewer")
        self.resize(1000, 700)
        
        # Main Layout
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        layout = QHBoxLayout(central_widget)
        
        splitter = QSplitter(Qt.Orientation.Horizontal)
        layout.addWidget(splitter)
        
        # Left Panel: Tree
        self.tree = QTreeWidget()
        self.tree.setHeaderLabel("Firmware Tables")
        self.tree.itemClicked.connect(self.on_item_clicked)
        splitter.addWidget(self.tree)
        
        # Right Panel: Tabs
        self.tabs = QTabWidget()
        splitter.addWidget(self.tabs)
        
        # Hex View Tab
        self.hex_view = HexViewer()
        self.tabs.addTab(self.hex_view, "Hex View")
        
        # Parsed View Tab
        self.parsed_view = QTextEdit()
        self.parsed_view.setReadOnly(True)
        self.parsed_view.setFont(QFont("Consolas", 10))
        self.tabs.addTab(self.parsed_view, "Parsed View")
        
        # Set splitter ratio
        splitter.setStretchFactor(0, 1)
        splitter.setStretchFactor(1, 3)

        self.load_data()

    def load_data(self):
        # Root for ACPI
        acpi_root = QTreeWidgetItem(self.tree, ["ACPI Tables"])
        
        try:
            tables = firmware_api.list_acpi_tables()
            if not tables:
                QMessageBox.warning(self, "Warning", "No ACPI tables found. Are you running as Admin?")
            
            for t in tables:
                item = QTreeWidgetItem(acpi_root, [t])
                item.setData(0, Qt.ItemDataRole.UserRole, ("ACPI", t))
                
        except Exception as e:
            QMessageBox.critical(self, "Error", f"Failed to list ACPI tables: {e}")

        # Root for SMBIOS
        smbios_root = QTreeWidgetItem(self.tree, ["SMBIOS Data"])
        # We process SMBIOS once to find structures
        smbios_data = firmware_api.get_smbios_data()
        if smbios_data:
            # Check for generic Windows header
            raw_header, offset = parsers.parse_raw_smbios_data_header(smbios_data)
            # If offset > 0, we found it, skip it.
            # If 0, maybe raw without header (unlikely on Windows API but safe to handle)
            current_off = offset
            
            while current_off < len(smbios_data):
                hdr, next_off = parsers.parse_smbios_structure(smbios_data, current_off)
                if not hdr:
                    break
                
                name = f"Type {hdr.type} (Handle {hdr.handle:04X})"
                
                # Friendly names for common types
                type_names = {
                    0: "BIOS Info", 1: "System Info", 2: "Baseboard",
                    3: "Chassis", 4: "Processor", 17: "Memory"
                }
                if hdr.type in type_names:
                    name += f" - {type_names[hdr.type]}"

                item = QTreeWidgetItem(smbios_root, [name])
                # Store (Type, Offset, Length, NextOffset)
                item.setData(0, Qt.ItemDataRole.UserRole, ("SMBIOS", current_off, hdr.length, next_off))
                current_off = next_off
        else:
             pass
             
        self.tree.expandAll()

    def on_item_clicked(self, item, column):
        data = item.data(0, Qt.ItemDataRole.UserRole)
        if not data:
            return
            
        cat = data[0]
        if cat == "ACPI":
            signature = data[1]
            raw_data = firmware_api.get_acpi_table(signature)
            self.show_acpi(signature, raw_data)
        elif cat == "SMBIOS":
            # ("SMBIOS", offset, length, next_off)
            offset = data[1]
            length = data[2]
            next_off = data[3]
            # We need the full data again, or cache it. 
            # Ideally cache it, but fetching again is cheap for this tool? 
            # Actually get_smbios_data calls kernel32, maybe better to cache.
            # For simplicity, call again.
            full_data = firmware_api.get_smbios_data()
            if full_data:
                structure_data = full_data[offset:next_off]
                self.show_smbios(structure_data, full_data, offset, length)

    def show_acpi(self, signature, data):
        self.hex_view.set_data(data)
        
        # Parse logic
        output = []
        output.append(f"ACPI Table: {signature}")
        output.append("="*30)
        try:
            header = parsers.parse_acpi_header(data)
            for field in header._fields:
                val = getattr(header, field)
                output.append(f"{field.capitalize()}: {val}")
        except Exception as e:
            output.append(f"Parsing error: {e}")
            
        self.parsed_view.setText("\n".join(output))

    def show_smbios(self, data, full_data, offset, formatted_len):
        self.hex_view.set_data(data)
        
        output = []
        try:
            hdr, _ = parsers.parse_smbios_structure(full_data, offset)
            strings = parsers.get_smbios_strings(full_data, offset, hdr.length)
            
            # Title
            type_names = {
                0: "BIOS Information", 1: "System Information", 2: "Baseboard Information",
                3: "Chassis Information", 4: "Processor Information", 17: "Memory Device"
            }
            title = f"SMBIOS Type {hdr.type}"
            if hdr.type in type_names:
                title += f" - {type_names[hdr.type]}"
            
            output.append(title)
            output.append(f"Handle: 0x{hdr.handle:04X}")
            output.append(f"Length: {hdr.length}")
            output.append("=" * 40)
            
            # Detailed Info
            details = parsers.get_parsed_smbios_info(hdr.type, full_data, offset, hdr.length, strings)
            
            if details:
                for key, val in details:
                    output.append(f"{key:25}: {val}")
            else:
                # Fallback
                if strings:
                    output.append("Strings:")
                    for i, s in enumerate(strings):
                        output.append(f"  {i+1}: {s}")
                else:
                    output.append("No strings.")
                
        except Exception as e:
            output.append(f"Error: {e}")

        self.parsed_view.setText("\n".join(output))

def run_gui():
    app = QApplication(sys.argv)
    window = MainWindow()
    window.show()
    sys.exit(app.exec())

if __name__ == "__main__":
    run_gui()
