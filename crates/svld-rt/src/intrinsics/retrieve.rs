use std::{ffi::c_void, ptr::NonNull};

use v8::{Global, Local};

/// Retrieves the intrinsic data from the intrinsics object stored in data slot 1.
pub fn retrieve_intrinsic<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &'static str,
) -> Option<Local<'s, v8::Value>> {
    let inner = scope.get_data(1);
    if inner.is_null() {
        return None;
    }

    let data = unsafe { NonNull::new_unchecked(inner as *mut v8::Value) };
    let gintrinsics = unsafe { Global::from_raw(scope, data) };

    let preserved = gintrinsics.clone();
    let intrinsics = Local::new(scope, gintrinsics);
    scope.set_data(1, preserved.into_raw().as_ptr() as *mut c_void);

    Some(
        intrinsics
            .cast::<v8::Object>()
            .get(scope, v8::String::new(scope, name)?.cast())?,
    )
}
