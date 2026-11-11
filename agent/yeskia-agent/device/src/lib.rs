//! Fixed-path, one-shot helper. No PIM exported; no command output written to debug logs.
#![no_std]
#![no_main]
extern crate alloc;
use alloc::{string::String, vec, vec::Vec};
use symbian::fs::{self, ShimFs, Utf16Path};
use yeskia_agent_protocol::{AppRecord, SystemApi, MAX_DOCUMENT, MAX_PAGE};
#[repr(C)]
#[derive(Clone, Copy)]
struct Row {
    uid: u32,
    len: i32,
    caption: [u16; 64],
    truncated: i32,
}
unsafe extern "C" {
    fn yeskia_apps_page(
        offset: i32,
        cap: i32,
        rows: *mut Row,
        count: *mut i32,
        more: *mut i32,
    ) -> i32;
}
struct System;
impl SystemApi for System {
    fn apps(&mut self, offset: u32, limit: u16) -> Result<(Vec<AppRecord>, Option<u32>), i32> {
        let mut rows = [Row {
            uid: 0,
            len: 0,
            caption: [0; 64],
            truncated: 0,
        }; MAX_PAGE as usize];
        let (mut count, mut more) = (0, 0);
        let rc = unsafe {
            yeskia_apps_page(
                offset as i32,
                limit as i32,
                rows.as_mut_ptr(),
                &mut count,
                &mut more,
            )
        };
        if rc != 0 {
            return Err(rc);
        }
        if count < 0 || count > limit as i32 {
            return Err(-6);
        }
        let mut entries = Vec::new();
        for row in &rows[..count as usize] {
            if row.len < 0 || row.len > 64 {
                return Err(-6);
            }
            entries.push(AppRecord {
                uid: row.uid,
                caption: String::from_utf16_lossy(&row.caption[..row.len as usize]),
                caption_truncated: row.truncated != 0,
            });
        }
        Ok((
            entries,
            if more != 0 {
                Some(offset + count as u32)
            } else {
                None
            },
        ))
    }
    fn heap_kib(&self) -> Option<u32> {
        symbian::mem::heap_used_kb().ok()
    }
}
struct Helper;
impl Helper {
    fn new() -> Self {
        let _ = Self::execute();
        Self
    }
    fn execute() -> symbian::Result<()> {
        let mut fs = ShimFs;
        let request = Utf16Path::new("C:\\Data\\_yeskia\\request.json")?;
        // Bound the read itself: a size check alone would race a concurrently replaced file.
        let mut file = fs::File::open(&mut fs, &request, symbian::OpenMode::Read)?;
        let mut bytes = vec![0u8; MAX_DOCUMENT + 1];
        let count = file.read_fully(&mut bytes)?;
        if count > MAX_DOCUMENT {
            return Ok(());
        }
        bytes.truncate(count);
        drop(file);
        let request = match yeskia_agent_protocol::decode_request(&bytes) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        let reply = yeskia_agent_protocol::handle(
            &request,
            &mut System,
            option_env!("YESKIA_AGENT_BUILD").unwrap_or("unversioned"),
        );
        if let Ok(encoded) = yeskia_agent_protocol::encode_response(&reply) {
            let response = Utf16Path::new("C:\\Data\\_yeskia\\response.json")?;
            fs::write_atomic(&mut fs, &response, &encoded)?;
        }
        Ok(())
    }
}
impl symbian_app::DaemonApp for Helper {
    fn handle_raw(&mut self, _: &symbian_app::symbian_sys::ShimEvent) {}
    fn should_exit(&self) -> bool {
        true
    }
}
symbian_app::daemon_entry!(Helper::new());
