//! Read-only Windows counters for this diagnostic process, not the owner's GUI.
#![allow(unsafe_code)]
use serde_json::{Value, json};
use std::{collections::BTreeMap, ffi::c_void};

#[repr(C)]
#[derive(Default)]
struct FileTime {
    low: u32,
    high: u32,
}
impl FileTime {
    fn seconds(&self) -> f64 {
        ((u64::from(self.high) << 32) | u64::from(self.low)) as f64 / 1e7
    }
}
#[repr(C)]
#[derive(Default)]
struct Memory {
    cb: u32,
    faults: u32,
    peak_ws: usize,
    ws: usize,
    peak_paged: usize,
    paged: usize,
    peak_nonpaged: usize,
    nonpaged: usize,
    pagefile: usize,
    peak_pagefile: usize,
    private: usize,
}
#[repr(C)]
#[derive(Default)]
struct Region {
    base: usize,
    allocation_base: usize,
    allocation_protect: u32,
    partition: u16,
    size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetProcessTimes(
        process: *mut c_void,
        created: *mut FileTime,
        exited: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn K32GetProcessMemoryInfo(process: *mut c_void, counters: *mut Memory, size: u32) -> i32;
    fn VirtualQuery(address: *const c_void, info: *mut Region, size: usize) -> usize;
}

pub(super) fn cpu() -> f64 {
    let (mut c, mut e, mut k, mut u) = Default::default();
    // SAFETY: a process pseudo-handle and four live, correctly sized output structs.
    assert_ne!(unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) }, 0);
    k.seconds() + u.seconds()
}

pub(super) fn memory() -> Value {
    let size = size_of::<Memory>() as u32;
    let mut m = Memory { cb: size, ..Default::default() };
    // SAFETY: the output struct matches PROCESS_MEMORY_COUNTERS_EX, with its exact size.
    assert_ne!(unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut m, size) }, 0);
    let mut regions = BTreeMap::<String, usize>::new();
    let mut address = 0usize;
    loop {
        let mut r = Region::default();
        // SAFETY: VirtualQuery inspects this process's address map; it never dereferences address.
        if unsafe { VirtualQuery(address as *const c_void, &mut r, size_of::<Region>()) } == 0 {
            break;
        }
        if r.state == 0x1000 {
            *regions.entry(format!("type_{:x}/protect_{:x}", r.kind, r.protect)).or_default() +=
                r.size;
        }
        let Some(next) = r.base.checked_add(r.size).filter(|next| *next > address) else { break };
        address = next;
    }
    json!({"private_bytes":m.private,"working_set_bytes":m.ws,"peak_working_set_bytes":m.peak_ws,"committed_regions":regions})
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetTopWindow(window: *mut c_void) -> *mut c_void;
    fn GetWindow(window: *mut c_void, command: u32) -> *mut c_void;
    fn GetWindowThreadProcessId(window: *mut c_void, process: *mut u32) -> u32;
    fn IsIconic(window: *mut c_void) -> i32;
}
pub(super) fn minimized() -> bool {
    // SAFETY: read-only enumeration of window handles; no application is controlled.
    unsafe {
        let mut w = GetTopWindow(std::ptr::null_mut());
        while !w.is_null() {
            let mut pid = 0;
            GetWindowThreadProcessId(w, &mut pid);
            if pid == std::process::id() && IsIconic(w) != 0 {
                return true;
            }
            w = GetWindow(w, 2);
        }
    }
    false
}
