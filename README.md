# ACPI & SMBIOS Dumper Tool

A Windows-based utility for inspecting ACPI tables and SMBIOS data. This tool provides both a Command Line Interface (CLI) and a Graphical User Interface (GUI) to easily dump and view system firmware information.

## Features

-   **ACPI Tables**: List all available ACPI tables and dump specific tables by signature (e.g., FACP, DSDT).
-   **SMBIOS Data**: Parse and display SMBIOS structures (Type 0, 1, 2, 3, 4, 17, and more).
-   **Hex & ASCII View**: Detailed hex dump views for analysis.
-   **GUI Mode**: specific user-friendly interface built with PyQt6.
-   **Rich CLI**: Colored and formatted output in the terminal (requires `rich`).

## Prerequisites

-   **Operating System**: Windows 10/11 (x64 recommended).
-   **Python**: Python 3.8 or newer.
-   **Privileges**: **Administrator privileges are REQUIRED** to access firmware tables via Windows APIs.

## Installation

1.  Clone or download this repository.
2.  Install the required dependencies:

    ```bash
    pip install -r requirements.txt
    ```

    *Note: `rich` is used for pretty CLI output, and `PyQt6` is required for the GUI.*

## Usage

**Important: You must run your terminal or IDE as Administrator.**

### Command Line Interface (CLI)

Run `cli_main.py` with the following arguments:

#### List ACPI Tables
Lists all ACPI tables found on the system.
```bash
python cli_main.py --acpi
```

#### Dump a Specific ACPI Table
Dumps the content of a specific table (e.g., `FACP`).
```bash
python cli_main.py --table FACP
```

#### Dump SMBIOS Data
Parses and displays SMBIOS structures.
```bash
python cli_main.py --smbios
```

### Graphical User Interface (GUI)

Launch the GUI to browse tables interactively.
```bash
python cli_main.py --gui
```

## Troubleshooting

-   **"Access Denied" or Empty Lists**: Ensure you are running the command prompt as **Administrator**. The Windows API `EnumSystemFirmwareTables` returns zero if the process lacks sufficient privileges.
-   **"Module not found" errors**: Make sure you have installed `requirements.txt`.
