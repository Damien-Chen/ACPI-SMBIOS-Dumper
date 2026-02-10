# ACPI & SMBIOS Dumper

A Windows-based utility for inspecting ACPI tables and SMBIOS data. Built with Rust and `eframe` (egui) for a fast, native graphical user interface.

## Features

-   **ACPI Tables**: List all available ACPI tables (including duplicates like SSDTs) using a combined Registry and API enumeration.
-   **SMBIOS Data**: Parse and display SMBIOS structures (Type 0, 1, 2, 3, 4, 17, and more).
-   **Enhanced XSDT View**: Displays physical addresses and table signatures for XSDT entries with FADT cross-referencing.
-   **Hex & ASCII View**: Detailed hex dump views for in-depth analysis.
-   **Exporting**: Export raw binary data for individual tables or all discovered ACPI tables at once.

## Prerequisites

-   **Operating System**: Windows 10/11 (x64).
-   **Privileges**: **Administrator privileges are REQUIRED** to access firmware tables via Windows APIs.

## Build Instructions

1.  Install Rust: https://rustup.rs/
2.  Clone this repository.
3.  Build the project:

    ```bash
    cargo build --release
    ```

    The binary will be located at `target/release/acpi-smbios-dumper.exe`.

## Usage

**Important: You must run the application as Administrator.**

Simply launch the executable:
```bash
acpi-smbios-dumper.exe
```

The application will open a window where you can browse and inspect ACPI and SMBIOS data.

## License

Licensed under the [MIT License](https://opensource.org/license/mit/). You are free to use, modify, and redistribute the software with proper attribution.
