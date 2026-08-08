// Demo capability plugin for cherry-quickjs.
//
// String ABI: the host calls alloc, writes utf8 input, calls f(ptr, len).
// f returns the output as ((ptr as i64) << 32) | len. Allocations leak;
// plugin instances are short-lived.

use sha2::{Digest, Sha256};

#[no_mangle]
pub extern "C" fn alloc(len: i32) -> i32 {
    let mut buf = Vec::<u8>::with_capacity(len as usize);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr as i32
}

fn return_string(s: String) -> i64 {
    let bytes = s.into_bytes();
    let len = bytes.len();
    let ptr = alloc(len as i32);
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len) };
    ((ptr as i64) << 32) | len as i64
}

fn input_str<'a>(ptr: i32, len: i32) -> &'a str {
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

#[no_mangle]
pub extern "C" fn sha256(ptr: i32, len: i32) -> i64 {
    let digest = Sha256::digest(input_str(ptr, len).as_bytes());
    let hex: String = digest.iter().map(|b| format!("{:02x}", b)).collect();
    return_string(hex)
}

#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}
