//! The C++ runtime symbols OpenH264 needs in the static musl build, where
//! there is no libstdc++ (see .cargo/musl-cxx.sh): `new`/`delete` backed by
//! Rust's allocator. OpenH264 uses no exceptions, RTTI or STL.

use std::alloc::{Layout, alloc, dealloc};

/// Room in front of each block for its size (keeps 16-byte alignment).
const HEADER: usize = 16;

unsafe fn allocate(size: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(size + HEADER, HEADER) else { std::process::abort() };
    // SAFETY: the layout is non-zero sized (it includes the header).
    unsafe {
        let base = alloc(layout);
        if base.is_null() {
            std::process::abort();
        }
        base.cast::<usize>().write(size);
        base.add(HEADER)
    }
}

unsafe fn release(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: `ptr` came from `allocate`, which put the size in front.
    unsafe {
        let base = ptr.sub(HEADER);
        let size = base.cast::<usize>().read();
        dealloc(base, Layout::from_size_align_unchecked(size + HEADER, HEADER));
    }
}

/// operator new(size_t)
#[unsafe(no_mangle)]
unsafe extern "C" fn _Znwm(size: usize) -> *mut u8 {
    unsafe { allocate(size) }
}

/// operator new[](size_t)
#[unsafe(no_mangle)]
unsafe extern "C" fn _Znam(size: usize) -> *mut u8 {
    unsafe { allocate(size) }
}

/// operator delete(void*)
#[unsafe(no_mangle)]
unsafe extern "C" fn _ZdlPv(ptr: *mut u8) {
    unsafe { release(ptr) }
}

/// operator delete[](void*)
#[unsafe(no_mangle)]
unsafe extern "C" fn _ZdaPv(ptr: *mut u8) {
    unsafe { release(ptr) }
}

/// operator delete(void*, size_t)
#[unsafe(no_mangle)]
unsafe extern "C" fn _ZdlPvm(ptr: *mut u8, _size: usize) {
    unsafe { release(ptr) }
}

/// operator delete[](void*, size_t)
#[unsafe(no_mangle)]
unsafe extern "C" fn _ZdaPvm(ptr: *mut u8, _size: usize) {
    unsafe { release(ptr) }
}

/// Called if a pure virtual method is ever invoked (a bug in the library).
#[unsafe(no_mangle)]
extern "C" fn __cxa_pure_virtual() -> ! {
    std::process::abort()
}
