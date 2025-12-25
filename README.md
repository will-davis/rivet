# Rivet

Rivet is a high-performance, Master File Table (MFT) based file search utility for Windows, built in Rust. It provides near-instantaneous indexing of millions of files by reading the NTFS MFT directly and monitors for real-time changes using the Update Sequence Number (USN) Journal.

## Key Features

- **Blazing Fast Indexing:** Traversing the $MFT at the volume level to build a complete file index in seconds.
- **Real-time Monitoring:** Continuous monitoring of the USN Journal to reflect file additions, deletions, and modifications instantly.
- **Background Metadata Fetching:** File sizes and timestamps are fetched in background threads to keep the UI responsive during massive index builds.
- **Native Action Buttons:**
  - 🚀 **Open/Run:** Execute files or open them with their default associated applications.
  - 📂 **Locate in Explorer:** Open the parent folder and automatically highlight the selected file.
- **Clean UI:** A modern, resizable table interface with column sorting and truncation for long paths.
- **Stand-alone Executable:** Launches without a console window (in release mode) and includes a custom application icon.


https://github.com/user-attachments/assets/c21b092c-5488-4683-9947-05fbdb383098


## Prerequisites

- **Windows 10/11**
- **Administrator Privileges:** Direct volume access (required for MFT/USN Journal reading) requires running the application as an administrator.
- **Rust Toolchain:** To build from source.

## Building

To build the release version of Rivet:

```powershell
cargo build --release
```

The compiled binary will be located at `target/release/rivet.exe`.

## Technical Details

- **MFT Enumeration:** Uses `FSCTL_ENUM_USN_DATA` for efficient enumeration of disk contents.
- **USN Monitoring:** Uses `FSCTL_READ_USN_JOURNAL` to track live filesystem changes.
- **GUI Engine:** Powered by [eframe](https://github.com/emilk/egui/tree/master/crates/eframe) and [egui](https://github.com/emilk/egui).
- **Concurrency:** Leverages `DashMap` for high-performance concurrent access to the file record cache.
- **Windows Integration:** Utilizes the `windows-rs` crate for direct interaction with Win32 APIs (Shell, Ioctl, Storage).

## License

MIT
