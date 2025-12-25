use crate::mft_indexer::{Indexer, FileRecord};
use usn_journal_rs::journal::UsnJournal;
use usn_journal_rs::volume::Volume;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use windows::Win32::Storage::FileSystem::{GetFileAttributesExW, GetFileExInfoStandard, WIN32_FILE_ATTRIBUTE_DATA};
use windows::core::HSTRING;

pub struct Monitor {
    indexer: Arc<Indexer>,
}

impl Monitor {
    pub fn new(indexer: Arc<Indexer>) -> Self {
        Self { indexer }
    }

    pub fn start_monitoring(&self, drive_letter: char, token: &CancellationToken) -> anyhow::Result<()> {
        let volume = Volume::from_drive_letter(drive_letter)?;
        let journal = UsnJournal::new(&volume);
        
        loop {
            if token.is_cancelled() {
                return Ok(());
            }

            // Re-creating iterator to keep polling from "current"
            if let Ok(iter) = journal.iter() {
                for record in iter {
                    if let Ok(entry) = record {
                        let mut size = 0;
                        if !entry.is_dir() {
                            let path = self.indexer.get_full_path(entry.fid, drive_letter);
                            let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
                            unsafe {
                                if GetFileAttributesExW(&HSTRING::from(path), GetFileExInfoStandard, &mut data as *mut _ as *mut _).is_ok() {
                                    size = ((data.nFileSizeHigh as u64) << 32) | (data.nFileSizeLow as u64);
                                }
                            }
                        }

                        let modified = entry.time
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| (d.as_secs() + 11_644_473_600) * 10_000_000)
                                    .unwrap_or(0) as i64;

                        let file_record = FileRecord {
                            id: entry.fid,
                            parent_id: entry.parent_fid,
                            name: entry.file_name.to_string_lossy().into_owned(),
                            size,
                            modified,
                            is_dir: entry.is_dir(),
                        };

                        self.indexer.records.insert(file_record.id, file_record);
                    }
                }
            }
            
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    }
}
