#![feature(allocator_api)]
extern crate libc;

use std::heap::Alloc;
use std::heap::Layout;
use std::heap::AllocErr;

/// To adhere to the rule that all calls to malloc, realloc, and free
/// be redirected to their "unexec_"-prefixed variants, this crate
/// provides a custom system allocator that performs such a mapping.

extern "C" {
    fn unexec_malloc(size: libc::size_t) -> *mut libc::c_void;
    fn unexec_realloc(old_ptr: *mut libc::c_void, new_size: libc::size_t) -> *mut libc::c_void;
    fn unexec_free(ptr: *mut libc::c_void);
}

pub struct OsxUnexecAlloc;

unsafe impl<'a> Alloc for &'a OsxUnexecAlloc {
    unsafe fn alloc(&mut self, layout: Layout) -> Result<*mut u8, AllocErr> {
        let addr = unexec_malloc(layout.size() as libc::size_t);
        if addr.is_null() {
            return Err(AllocErr::Exhausted { request: layout });
        }

        assert_eq!(addr as usize & (layout.align() - 1), 0);
        Ok(addr as *mut u8)
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        assert_eq!(ptr as usize & (layout.align() - 1), 0);
        unexec_free(ptr as *mut libc::c_void)
    }

    unsafe fn realloc(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        new_layout: Layout,
    ) -> Result<*mut u8, AllocErr> {
        let addr = unexec_realloc(ptr as *mut libc::c_void, new_layout.size() as libc::size_t);
        if addr.is_null() {
            return Err(AllocErr::Exhausted { request: new_layout });
        }

        assert_eq!(addr as usize & (new_layout.align() - 1), 0);
        Ok(addr as *mut u8)
    }
}
