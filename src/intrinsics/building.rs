use std::mem;

use v8::{
    GetPropertyNamesArgs, Global, Isolate, Local, PinScope, Platform, Promise, PromiseState,
    SharedRef,
};

use crate::{
    compile::compile_module,
    intrinsics::{self, files::get_intrinsics_file},
    scope_with_context,
};

/// Add a function. Example:
///
/// ```no_run
/// add_fn!(
///     let add = intrinsics::add,
///     in: intrinsics_obj,
///     scope: scope
/// );
/// ```
macro_rules! add_fn {
    (let $name:ident = $k:expr, in: $obj:ident, scope: $scope:expr) => {{
        let fnk = v8::Function::new($scope, $k).unwrap();
        let k = v8::String::new($scope, stringify!($name)).unwrap();
        $obj.set($scope, k.into(), fnk.into());
    }};
}

fn resolve_module_callback_intrinsics<'a>(
    context: v8::Local<'a, v8::Context>,
    specifier: v8::Local<'a, v8::String>,
    _import_attributes: v8::Local<'a, v8::FixedArray>,
    _referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'a, v8::Module>> {
    let scope = std::pin::pin!(unsafe { v8::CallbackScope::new(context) });
    let scope_rf = &mut scope.init();

    let specifier_str = specifier.to_rust_string_lossy(scope_rf);
    if !specifier_str.starts_with("intrinsics:") {
        // relative imports are not supported here
        // or rather because im lazy
        //
        // use:
        // import { something } from "intrinsics:someFile"
        // instead, looks cleaner aint it? no it doesnt
        return None;
    }

    let name = specifier_str
        .split_once(":")
        .unwrap_or(("intrinsics:", ""))
        .1;
    let Some(code) = get_intrinsics_file(name) else {
        return None;
    };

    let compiled = compile_module(scope_rf, code, &specifier_str);

    // essentially this shit is still managed by the v8 runtime, so
    // literally all we have to do is not make rust fury about the
    // `compiled` variable living only in this function scope (which is
    // false, because we pin it, and the rust compiler cannot reason
    // about such problems, since we're dealing with the bindings here)
    Some(unsafe { mem::transmute::<_, Local<'a, v8::Module>>(compiled) })
}

/// Build intrinsics and store them in a [`Global`]-sealed [`v8::Value`].
#[must_use]
pub fn build_intrinsics(
    platform: &SharedRef<Platform>,
    isolate: &mut Isolate,
) -> Global<v8::Value> {
    let (module, promised) = {
        scope_with_context!(
            isolate: isolate,
            let &mut scope,
            let context
        );

        let intrinsics_obj = v8::Object::new(scope);

        // fetch()
        add_fn!(
            let fetch = intrinsics::fetch,
            in: intrinsics_obj,
            scope: scope
        );

        context.global(scope).set(
            scope,
            v8::String::new(scope, "intrinsics").unwrap().cast(),
            intrinsics_obj.cast(),
        );

        let source = get_intrinsics_file("index").unwrap();

        let module = compile_module(scope, source, "index");
        module
            .instantiate_module(scope, resolve_module_callback_intrinsics)
            .unwrap();

        let promised = module
            .evaluate(scope)
            .expect("failed to evaluate")
            .cast::<Promise>();

        (Global::new(scope, module), Global::new(scope, promised))
    };

    // wait for the module to load
    while Platform::pump_message_loop(&platform, isolate, false) {}

    scope_with_context!(
        isolate: isolate,
        let &mut scope,
        let context
    );

    let module = Local::new(scope, module);
    let promised = Local::new(scope, promised);

    if matches!(promised.state(), PromiseState::Rejected) {
        panic!("failed to build intrinsics");
    }

    let namespace = module.get_module_namespace();
    Global::new(scope, namespace)
}

/// Extract intrinsics to the scope so it can be used by the user.
pub fn extract_intrinsics(
    scope: &PinScope,
    context_global: Local<v8::Object>,
    intrinsics: Global<v8::Value>,
) -> Option<()> {
    let intrinsics = Local::new(scope, intrinsics).cast::<v8::Object>();
    let names = intrinsics.get_own_property_names(scope, GetPropertyNamesArgs::default())?;

    for idx in 0..names.length() {
        let name = names.get_index(scope, idx)?;
        let item = intrinsics.get(scope, name)?;
        context_global.set(scope, name, item);
    }

    Some(())
}
