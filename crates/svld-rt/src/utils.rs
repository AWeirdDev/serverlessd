use std::ffi::c_void;

/// An owned string which is ready to be turned into v8 external data.
///
/// ```no_run
/// // assuming this is of lifetime 'static
/// let s: &'static OwnedStr = ...;
///
/// // you get a &str as data
/// let data = v8::External::new(
///     scope,
///     unsafe { self.get_static_binding_name() } as *const _ as *mut c_void
/// );
/// ```
///
/// # Memory safety
/// `Drop`.
pub struct OwnedStr {
    ptr: *const u8,
    len: usize,
}

impl OwnedStr {
    /// Creates an `OwnedStr` from a `String`.
    #[inline]
    pub fn new(name: String) -> Self {
        let name = name.into_boxed_str();

        let len = name.len();
        let name_ptr = Box::into_raw(name);

        Self {
            ptr: name_ptr as *mut u8,
            len,
        }
    }

    #[inline(always)]
    pub const fn as_str(&self) -> &str {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        unsafe { core::str::from_utf8_unchecked(slice) }
    }

    #[inline(always)]
    pub const unsafe fn from_void_ptr(ptr: *mut c_void) -> &'static str {
        unsafe { &*(ptr as *const Self) }.as_str()
    }
}

impl From<String> for OwnedStr {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for OwnedStr {
    #[inline(always)]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

unsafe impl Sync for OwnedStr {}
unsafe impl Send for OwnedStr {}

impl Drop for OwnedStr {
    fn drop(&mut self) {
        let slice_ptr = core::ptr::slice_from_raw_parts(self.ptr, self.len);
        let str_ptr = slice_ptr as *mut str; // a fat pointer
        let _ = unsafe { Box::from_raw(str_ptr) };
    }
}
