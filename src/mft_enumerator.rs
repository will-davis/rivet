use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::Win32::Foundation::{HANDLE, ERROR_HANDLE_EOF, GENERIC_READ};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_SHARE_DELETE,
    OPEN_EXISTING, FILE_ATTRIBUTE_DIRECTORY, FILE_FLAGS_AND_ATTRIBUTES,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V0, USN_RECORD_V2,
};
use windows::core::HSTRING;

pub struct MftEntry {
    pub fid: u64,
    pub parent_fid: u64,
    pub name: String,
    pub modified: i64,
    pub is_dir: bool,
}

pub struct MftEnumerator {
    handle: HANDLE,
}

impl MftEnumerator {
    pub fn new(drive_letter: char) -> anyhow::Result<Self> {
        let drive_path = format!("\\\\.\\{}:", drive_letter);
        let handle = unsafe {
            CreateFileW(
                &HSTRING::from(drive_path),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )?
        };
        Ok(Self { handle })
    }

    pub fn iter(&self) -> MftIter {
        MftIter {
            handle: self.handle,
            next_start_fid: 0,
            buffer: vec![0u8; 128 * 1024], // Increased buffer size
            offset: 0,
            bytes_read: 0,
        }
    }
}

impl Drop for MftEnumerator {
    fn drop(&mut self) {
        unsafe { let _ = windows::Win32::Foundation::CloseHandle(self.handle); }
    }
}

pub struct MftIter {
    handle: HANDLE,
    next_start_fid: u64,
    buffer: Vec<u8>,
    offset: usize,
    bytes_read: u32,
}

impl Iterator for MftIter {
    type Item = anyhow::Result<MftEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.offset < self.bytes_read as usize {
                let record = unsafe {
                    &*(self.buffer.as_ptr().add(self.offset) as *const USN_RECORD_V2)
                };
                self.offset += record.RecordLength as usize;

                let name_len = record.FileNameLength as usize / 2;
                let name_ptr = record.FileName.as_ptr();
                let name_slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len) };
                let name = OsString::from_wide(name_slice).to_string_lossy().into_owned();

                return Some(Ok(MftEntry {
                    fid: record.FileReferenceNumber,
                    parent_fid: record.ParentFileReferenceNumber,
                    name,
                    modified: record.TimeStamp,
                    is_dir: (record.FileAttributes & FILE_ATTRIBUTE_DIRECTORY.0) != 0,
                }));
            }

            // Need to read more data
            let mft_enum_data = MFT_ENUM_DATA_V0 {
                StartFileReferenceNumber: self.next_start_fid,
                LowUsn: 0,
                HighUsn: i64::MAX,
            };

            let mut bytes_returned = 0u32;
            let success = unsafe {
                DeviceIoControl(
                    self.handle,
                    FSCTL_ENUM_USN_DATA,
                    Some(&mft_enum_data as *const _ as _),
                    std::mem::size_of::<MFT_ENUM_DATA_V0>() as u32,
                    Some(self.buffer.as_mut_ptr() as _),
                    self.buffer.len() as u32,
                    Some(&mut bytes_returned),
                    None,
                )
            };

            if let Err(e) = success {
                if e.code() == ERROR_HANDLE_EOF.into() {
                    return None;
                }
                return Some(Err(anyhow::anyhow!("DeviceIoControl failed at FID 0x{:x}: {}", self.next_start_fid, e)));
            }

            if bytes_returned < 8 {
                return None; // Should at least have the next start FID
            }

            self.bytes_read = bytes_returned;
            self.next_start_fid = unsafe { *(self.buffer.as_ptr() as *const u64) };
            self.offset = 8; // Skip the next start FID
            
            // If we only got the next FID but no records, loop again
            if self.offset >= self.bytes_read as usize {
                continue;
            }
        }
    }
}
