//! Link shim for prebuilt ONNX Runtime (`ort` + `download-binaries`) on glibc < 2.38.
//!
//! Recent `libonnxruntime.a` builds reference C23 `strto*` symbols (`__isoc23_strtol`, …)
//! that only exist in glibc ≥ 2.38 (Ubuntu 24.04+). Ubuntu 22.04 / Debian 12 ship 2.35,
//! so the final link fails with `undefined symbol: __isoc23_strtoll`.
//!
//! These forwards call the classic libc entry points. Safe for ONNX config parsing
//! (decimal/hex); the only C23 difference is a `0b` binary prefix we do not need.

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_long, c_longlong, c_ulong, c_ulonglong};

type LocaleT = *mut c_void;

unsafe extern "C" {
    fn strtol(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_long;
    fn strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_longlong;
    fn strtoul(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_ulong;
    fn strtoull(nptr: *const c_char, endptr: *mut *mut c_char, base: c_int) -> c_ulonglong;
    fn strtoll_l(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: c_int,
        locale: LocaleT,
    ) -> c_longlong;
    fn strtoull_l(
        nptr: *const c_char,
        endptr: *mut *mut c_char,
        base: c_int,
        locale: LocaleT,
    ) -> c_ulonglong;
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtol(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_long {
    strtol(nptr, endptr, base)
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtoll(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_longlong {
    strtoll(nptr, endptr, base)
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtoul(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulong {
    strtoul(nptr, endptr, base)
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtoull(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
) -> c_ulonglong {
    strtoull(nptr, endptr, base)
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtoll_l(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
    locale: LocaleT,
) -> c_longlong {
    strtoll_l(nptr, endptr, base, locale)
}

#[no_mangle]
pub unsafe extern "C" fn __isoc23_strtoull_l(
    nptr: *const c_char,
    endptr: *mut *mut c_char,
    base: c_int,
    locale: LocaleT,
) -> c_ulonglong {
    strtoull_l(nptr, endptr, base, locale)
}

/// Keep shim symbols reachable under LTO so the linker can resolve ort's refs.
#[used]
static KEEP_ISOC23_SHIMS: [*const (); 6] = [
    __isoc23_strtol as *const (),
    __isoc23_strtoll as *const (),
    __isoc23_strtoul as *const (),
    __isoc23_strtoull as *const (),
    __isoc23_strtoll_l as *const (),
    __isoc23_strtoull_l as *const (),
];
