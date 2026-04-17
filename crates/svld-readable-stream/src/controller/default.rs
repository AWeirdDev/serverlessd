use std::ffi::c_void;

use v8::{External, FunctionCallbackArguments, Global, Local, Object, PinScope, ReturnValue};

use svld_language::{ThrowException, throw};

use crate::state::{ReadableStreamState, StreamInternalState};
