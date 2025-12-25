use crate::mft_enumerator::MftEnumerator;
use dashmap::DashMap;
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::{HANDLE, CloseHandle, GENERIC_READ};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{FSCTL_QUERY_USN_JOURNAL, USN_JOURNAL_DATA_V0};
use windows::Win32::System::IO::DeviceIoControl;
use windows::core::HSTRING;

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: u64,
    pub parent_id: u64,
    pub name: String,
    pub size: u64,
    pub modified: i64,
    pub is_dir: bool,
}

pub struct Indexer {
    // FileId -> FileRecord
    pub records: DashMap<u64, FileRecord>,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            records: DashMap::new(),
        }
    }

    pub fn index_volume(&self, drive_letter: char, token: &CancellationToken) -> anyhow::Result<()> {
        // Ensure USN journal is active
        let volume_path = format!("\\\\.\\{}:", drive_letter);
        let volume_handle = unsafe {
            CreateFileW(
                &HSTRING::from(volume_path),
                GENERIC_READ.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                Default::default(),
                HANDLE::default(),
            )?
        };

        let mut usn_journal_data = USN_JOURNAL_DATA_V0::default();
        let mut bytes_returned = 0;

        let result = unsafe {
            DeviceIoControl(
                volume_handle,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some(&mut usn_journal_data as *mut _ as *mut std::ffi::c_void),
                std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
                Some(&mut bytes_returned),
                None,
            )
        };

        unsafe { let _ = CloseHandle(volume_handle); }

        if let Err(e) = result {
            anyhow::bail!("Failed to query USN journal for volume {}:\\: {}", drive_letter, e);
        }

        let enumerator = MftEnumerator::new(drive_letter)?;
        
        for entry in enumerator.iter() {
            if token.is_cancelled() {
                return Ok(());
            }
            let entry = entry?;
            
            let record = FileRecord {
                id: entry.fid,
                parent_id: entry.parent_fid,
                name: entry.name,
                size: 0, // Will be fetched later
                modified: entry.modified,
                is_dir: entry.is_dir,
            };
            
            self.records.insert(record.id, record);
        }
        
        Ok(())
    }

    pub fn fetch_sizes(&self, drive_letter: char, token: &CancellationToken) {
        use windows::Win32::Storage::FileSystem::{GetFileAttributesExW, GetFileExInfoStandard, WIN32_FILE_ATTRIBUTE_DATA};
        use windows::core::HSTRING;

        println!("Indexing complete. Starting metadata fetch for {} items...", self.records.len());

        // Process in large chunks to avoid holding million-entry vectors
        let all_ids: Vec<u64> = self.records.iter().map(|r| *r.key()).collect();
        
        for (i, id) in all_ids.iter().enumerate() {
            if token.is_cancelled() { break; }
            if i % 10000 == 0 && i > 0 {
                println!("Metadata progress: {}/{}", i, all_ids.len());
            }

            // 1. Check if we need to fetch (using read lock)
            let (is_dir, current_size) = if let Some(r) = self.records.get(id) {
                (r.is_dir, r.size)
            } else {
                continue;
            };

            if is_dir || current_size > 0 {
                continue;
            }

            // 2. Build path WITHOUT holding a lock on the record we're about to update
            let path = self.get_full_path(*id, drive_letter);
            
            // 3. System call
            let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
            let size = unsafe {
                if GetFileAttributesExW(&HSTRING::from(path), GetFileExInfoStandard, &mut data as *mut _ as *mut _).is_ok() {
                    Some(((data.nFileSizeHigh as u64) << 32) | (data.nFileSizeLow as u64))
                } else {
                    None
                }
            };

            // 4. Update (using write lock)
            if let Some(s) = size {
                if let Some(mut item) = self.records.get_mut(id) {
                    item.size = s;
                }
            }
        }
    }

    pub fn get_full_path(&self, id: u64, drive_letter: char) -> String {
        let mut components = Vec::new();
        let mut current_id = id;
        let mut visited = std::collections::HashSet::new();
        
        while let Some(record) = self.records.get(&current_id) {
            if !visited.insert(current_id) || visited.len() > 64 {
                break; 
            }

            components.push(record.name.clone());
            
            if record.parent_id == current_id || record.parent_id == 0 {
                break;
            }
            current_id = record.parent_id;
        }
        
        components.reverse();
        format!("{}:\\{}", drive_letter, components.join("\\"))
    }
}
