use v8::{PinScope, ScriptOrigin, script_compiler::Source};

pub fn resolve_module_callback<'a>(
    _context: v8::Local<'a, v8::Context>,
    _specifier: v8::Local<'a, v8::String>,
    _import_attributes: v8::Local<'a, v8::FixedArray>,
    _referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'a, v8::Module>> {
    None
}

pub fn compile_module<'s, K0: AsRef<str>, K1: AsRef<str>>(
    scope: &PinScope<'s, '_>,
    source: K0,
    name: K1,
    script_id: i32,
) -> Option<v8::Local<'s, v8::Module>> {
    let source_str = v8::String::new(scope, source.as_ref()).unwrap();
    let name_str = v8::String::new(scope, name.as_ref()).unwrap();

    let origin = ScriptOrigin::new(
        scope,
        name_str.into(),
        0,         // line offset
        0,         // column offset
        false,     // is_shared_cross_origin
        script_id, // script_id
        None,      // source_map_url
        false,     // is_opaque
        false,     // is_wasm
        true,      // is_module
        None,      // host_defined_options
    );

    let mut source = Source::new(source_str, Some(&origin));

    v8::script_compiler::compile_module(scope, &mut source)
}
