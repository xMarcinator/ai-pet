//! The Rust side of src/inspect.cpp.

use std::ffi::{c_char, c_void};

type Cb = extern "C" fn(*mut c_void, i32, *const c_char, usize, usize);

unsafe extern "C" {
    fn spike_inspect(data: *const u8, len: usize, cb: Cb, ctx: *mut c_void) -> i32;
}

#[derive(Default, Debug)]
pub struct Report {
    pub status: i32,
    /// name, state machines, animations
    pub artboards: Vec<(String, Vec<String>, Vec<String>)>,
    /// unique file name, in-band byte count, has a CDN uuid
    pub assets: Vec<(String, usize, bool)>,
    pub view_models: usize,
}

extern "C" fn cb(ctx: *mut c_void, kind: i32, text: *const c_char, len: usize, value: usize) {
    let report = unsafe { &mut *(ctx as *mut Report) };
    let text = if len == 0 {
        String::new()
    } else {
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text as *const u8, len) }).into_owned()
    };
    match kind {
        0 => report.artboards.push((text, Vec::new(), Vec::new())),
        1 => report.artboards.last_mut().unwrap().1.push(text),
        2 => report.artboards.last_mut().unwrap().2.push(text),
        3 => report.assets.push((text, value, false)),
        4 => report.view_models = value,
        5 => report.assets.last_mut().unwrap().2 = true,
        _ => {}
    }
}

pub fn inspect(bytes: &[u8]) -> Report {
    let mut report = Report::default();
    report.status = unsafe { spike_inspect(bytes.as_ptr(), bytes.len(), cb, &mut report as *mut _ as *mut c_void) };
    report
}
