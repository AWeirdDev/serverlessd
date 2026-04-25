use bytes::Bytes;
use v8::{Local, PinScope, Value};

/// Gets the bytes of some data, usually from a response body.
pub fn get_bytes<'s>(scope: &PinScope<'s, '_>, source: Local<'s, Value>) -> Option<Bytes> {
    let rs = source.cast::<v8::Object>();

    if rs.is_string() {
        let s = rs.to_string(scope)?.to_rust_string_lossy(scope);
        Some(Bytes::from(s))
    } else if rs.is_array_buffer_view() {
        let ab = rs.cast::<v8::ArrayBuffer>();

        let data = ab.data()?;
        let byte_ln = ab.byte_length();

        let slice = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_ln) };

        Some(Bytes::from_owner(slice))
    } else if rs.is_array_buffer() {
        let ab = rs.cast::<v8::ArrayBuffer>();

        let data = ab.data()?;
        let byte_ln = ab.byte_length();

        let slice = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_ln) };

        Some(Bytes::from_owner(slice))
    } else {
        None
    }
}
