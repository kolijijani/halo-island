//! The sysctl, memory and uptime readings the System page's reader uses
//! (`services::sysmon`). Today's once-a-second tiles that sampled CPU, disk
//! and the rest left with them (Today shows the news since 0.2.31).
//!
//! * Memory: Activity Monitor's split (app memory, wired, compressed, cached).
//! * Uptime: since the Mac started (`kern.boottime`).

use std::ffi::{c_char, c_void, CStr};
use std::time::{SystemTime, UNIX_EPOCH};

const HOST_VM_INFO64: i32 = 4;

/// `vm_statistics64` (mach/vm_statistics.h).
#[repr(C, align(8))]
#[derive(Default)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

#[repr(C)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i32,
}

extern "C" {
    fn mach_host_self() -> u32;
    fn host_statistics64(host: u32, flavor: i32, info: *mut i32, count: *mut u32) -> i32;
    fn sysctlbyname(name: *const c_char, oldp: *mut c_void, oldlenp: *mut usize, newp: *mut c_void, newlen: usize) -> i32;
}

/// Physical memory the way Activity Monitor splits it, in bytes (the System
/// page shows the parts).
pub(super) struct MemSplit {
    pub app: u64,
    pub wired: u64,
    pub compressed: u64,
    /// Cached files: file-backed and purgeable pages.
    pub cached: u64,
}

pub(super) fn memory_split() -> Option<MemSplit> {
    let vm = vm_stats()?;
    let page = sysctl_u64(c"hw.pagesize").unwrap_or(16384);
    let pages = |n: u32| n as u64 * page;
    Some(MemSplit {
        app: pages(vm.internal_page_count).saturating_sub(pages(vm.purgeable_count)),
        wired: pages(vm.wire_count),
        compressed: pages(vm.compressor_page_count),
        cached: pages(vm.external_page_count) + pages(vm.purgeable_count),
    })
}

fn vm_stats() -> Option<VmStatistics64> {
    let mut vm = VmStatistics64::default();
    let mut count = (std::mem::size_of::<VmStatistics64>() / 4) as u32;
    let r = unsafe { host_statistics64(mach_host_self(), HOST_VM_INFO64, &mut vm as *mut VmStatistics64 as *mut i32, &mut count) };
    (r == 0).then_some(vm)
}

pub(super) fn sysctl_u64(name: &CStr) -> Option<u64> {
    let mut v: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let r = unsafe { sysctlbyname(name.as_ptr(), &mut v as *mut u64 as *mut c_void, &mut len, std::ptr::null_mut(), 0) };
    match (r, len) {
        (0, 8) => Some(v),
        (0, 4) => Some(v & 0xffff_ffff),
        _ => None,
    }
}

pub(super) fn uptime_s() -> Option<u64> {
    let mut tv = Timeval { tv_sec: 0, tv_usec: 0 };
    let mut len = std::mem::size_of::<Timeval>();
    let r = unsafe { sysctlbyname(c"kern.boottime".as_ptr(), &mut tv as *mut Timeval as *mut c_void, &mut len, std::ptr::null_mut(), 0) };
    if r != 0 || tv.tv_sec <= 0 {
        return None;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((now - tv.tv_sec).max(0) as u64)
}

