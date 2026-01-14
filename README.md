# ACPI & SMBIOS Dumper

A Windows-based utility for inspecting ACPI tables and SMBIOS data. Built with Rust for performance and portability. This tool provides both a Command Line Interface (CLI) and a Graphical User Interface (GUI).

## Features

-   **ACPI Tables**: List all available ACPI tables and dump specific tables by signature (e.g., FACP, DSDT).
-   **SMBIOS Data**: Parse and display SMBIOS structures (Type 0, 1, 2, 3, 4, 17, and more).
-   **Hex & ASCII View**: Detailed hex dump views for analysis.
-   **GUI Mode**: Fast, native interface built with `egui`.
-   **CLI Mode**: Rich output in the terminal.

## Prerequisites

-   **Operating System**: Windows 10/11 (x64).
-   **Privileges**: **Administrator privileges are REQUIRED** to access firmware tables via Windows APIs.

## Build Instructions

1.  Install Rust (if not already installed): https://rustup.rs/
2.  Clone this repository.
3.  Build the project:

    ```bash
    cargo build --release
    ```

    The binary will be located at `target/release/acpi-smbios-dumper.exe`.

## Usage

**Important: You must run your terminal or IDE as Administrator.**

### Command Line Interface (CLI)

#### List ACPI Tables
Lists all ACPI tables found on the system.
```bash
acpi-smbios-dumper.exe --acpi
```

#### Dump a Specific ACPI Table
Dumps the content of a specific table (e.g., `FACP`).
```bash
acpi-smbios-dumper.exe --table FACP
```

#### Dump SMBIOS Data
Parses and displays SMBIOS structures.
```bash
acpi-smbios-dumper.exe --smbios
```

### Graphical User Interface (GUI)

Launch the GUI to browse tables interactively.
```bash
acpi-smbios-dumper.exe --gui
```

## Troubleshooting

-   **"Access Denied" or Empty Lists**: Ensure you are running as **Administrator**.
-   **Build Errors**: Ensure you have the MSVC build tools installed (usually comes with Rust on Windows).
