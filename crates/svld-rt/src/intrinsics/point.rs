/// Breaking point for debugging.
pub fn point(
    _scope: &mut v8::PinScope,
    _args: v8::FunctionCallbackArguments,
    _rv: v8::ReturnValue,
) {
    println!("POINT");
}
