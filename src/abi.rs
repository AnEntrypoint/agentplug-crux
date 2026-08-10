//! wasm32-wasip1-only: the agentplug plugin ABI boundary (plugin_call /
//! plugkit_alloc / plugkit_free) and the host filesystem imports the wasm
//! plugin path needs. Not compiled into the native CLI build.
#![cfg(target_arch = "wasm32")]

use std::alloc::{alloc, dealloc, Layout};
use std::mem;

use serde_json::Value;

#[link(wasm_import_module = "env")]
extern "C" {
    fn host_fs_read(path_ptr: *const u8, path_len: u32) -> u64;
    fn host_fs_readdir(path_ptr: *const u8, path_len: u32) -> u64;
    fn host_fs_stat(path_ptr: *const u8, path_len: u32) -> u64;
    fn host_cwd() -> u64;
}

#[no_mangle]
pub extern "C" fn plugkit_alloc(len: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    let layout = Layout::from_size_align(len as usize, mem::align_of::<u8>()).unwrap();
    unsafe { alloc(layout) as u32 }
}

#[no_mangle]
pub extern "C" fn plugkit_free(ptr: u32, len: u32) {
    if ptr == 0 || len == 0 {
        return;
    }
    let layout = Layout::from_size_align(len as usize, mem::align_of::<u8>()).unwrap();
    unsafe { dealloc(ptr as *mut u8, layout) };
}

pub fn read_str(ptr: u32, len: u32) -> String {
    if len == 0 {
        return String::new();
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        String::from_utf8_lossy(slice).into_owned()
    }
}

fn unpack_to_string(packed: u64) -> Option<String> {
    let p = (packed & 0xffff_ffff) as u32;
    let l = (packed >> 32) as u32;
    if p == 0 || l == 0 {
        return None;
    }
    let bytes = unsafe { Vec::from_raw_parts(p as *mut u8, l as usize, l as usize) };
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn return_bytes(bytes: Vec<u8>) -> u64 {
    if bytes.is_empty() {
        return 0;
    }
    let len = bytes.len();
    let ptr = plugkit_alloc(len as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len);
    }
    (ptr as u64 & 0xffff_ffff) | ((len as u64) << 32)
}

pub fn return_json(v: Value) -> u64 {
    return_bytes(v.to_string().into_bytes())
}

/// Reads a whole file through the host, sandboxed to the calling plugin
/// instance's project root (see agentplug-host's `sandboxed_guest_path`).
/// `None` on any failure -- missing file, path outside the sandbox, or a
/// non-UTF8 read.
pub fn host_read(path: &str) -> Option<String> {
    let packed = unsafe { host_fs_read(path.as_ptr(), path.len() as u32) };
    unpack_to_string(packed)
}

/// Lists direct children of `path` (names only, not full paths), or an
/// empty vec if the directory does not exist / is outside the sandbox.
pub fn host_readdir(path: &str) -> Vec<String> {
    let packed = unsafe { host_fs_readdir(path.as_ptr(), path.len() as u32) };
    match unpack_to_string(packed).and_then(|s| serde_json::from_str::<Value>(&s).ok()) {
        Some(Value::Array(arr)) => arr.into_iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

pub struct Stat {
    pub is_dir: bool,
    #[allow(dead_code)] // part of the host_fs_stat response shape; unused by scan today
    pub is_file: bool,
}

pub fn host_stat(path: &str) -> Option<Stat> {
    let packed = unsafe { host_fs_stat(path.as_ptr(), path.len() as u32) };
    let v: Value = unpack_to_string(packed).and_then(|s| serde_json::from_str(&s).ok())?;
    if v.is_null() {
        return None;
    }
    Some(Stat {
        is_dir: v.get("isDirectory").and_then(Value::as_bool).unwrap_or(false),
        is_file: v.get("isFile").and_then(Value::as_bool).unwrap_or(false),
    })
}

pub fn host_cwd_string() -> String {
    let packed = unsafe { host_cwd() };
    unpack_to_string(packed).unwrap_or_default()
}

#[no_mangle]
pub extern "C" fn plugin_call(verb_ptr: u32, verb_len: u32, body_ptr: u32, body_len: u32) -> u64 {
    let verb = read_str(verb_ptr, verb_len);
    let body_str = read_str(body_ptr, body_len);
    let body: Value = serde_json::from_str(&body_str).unwrap_or(serde_json::json!({}));

    match verb.as_str() {
        "scan" => crate::pipeline::handle_scan(&body),
        "capabilities" => return_json(serde_json::json!({
            "ok": true,
            "plugin": "crux",
            "verbs": ["scan", "capabilities"],
            "payload_field": { "scan": "dump" },
            "description": "Concentrates statistically rare/surprising material out of a project's own .jsonl trace files (session transcripts, workflow logs) into a small uninterpreted dump. scan takes an optional {\"root\": \"relative/path\"} (default: project root) and the same select_percentile/select_min/select_max/weights/smoothing/context_window knobs as the crux CLI.",
        })),
        _ => return_json(serde_json::json!({"ok": false, "error": "unknown_verb", "verb": verb})),
    }
}
