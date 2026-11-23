//! `tech.rws8.hitop.ffi.Native.call(byte[] op, byte[] args): byte[]`. Strings cross as UTF-8
//! bytes, never as JNI strings (modified UTF-8 mangles NUL and characters outside the BMP).

use jni::objects::{JByteArray, JClass};
use jni::sys::jbyteArray;
use jni::JNIEnv;

#[no_mangle]
pub extern "system" fn Java_tech_rws8_hitop_ffi_Native_call<'a>(mut env: JNIEnv<'a>, _class: JClass<'a>, op: JByteArray<'a>, args: JByteArray<'a>) -> jbyteArray {
    let read = |env: &mut JNIEnv<'a>, a: &JByteArray<'a>| env.convert_byte_array(a).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let op = read(&mut env, &op);
    let args = read(&mut env, &args);
    let out = crate::call_json(&op, &args);
    match env.byte_array_from_slice(out.as_bytes()) {
        Ok(a) => a.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
