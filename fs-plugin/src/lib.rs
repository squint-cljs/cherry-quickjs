// Filesystem capability plugin for cherry-quickjs, compiled to
// wasm32-wasip1. File access works only for directories the host
// preopens (the --allow flag). Same string ABI as demo-plugin.

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
pub extern "C" fn head(ptr: i32, len: i32) -> i64 {
    let path = input_str(ptr, len);
    return_string(match std::fs::read_to_string(path) {
        Ok(s) => s.lines().take(5).collect::<Vec<_>>().join("\n"),
        Err(e) => format!("error: {}: {}", path, e),
    })
}
