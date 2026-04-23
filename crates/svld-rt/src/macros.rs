/// Create a scope with context.
///
/// Example:
/// ```no_run
/// // get a &mut scope (creates a brand-new context)
/// scope_with_context!(
///     isolate: isolate,
///     let &mut scope,
///     let context
/// );
///
/// // get an owned scope (creates a brand-new context)
/// scope_with_context!(
///     isolate: isolate,
///     let scope,
///     let context
/// );
///
/// // enter an existing Global<Context> — does NOT create a new context
/// scope_with_context!(
///     isolate: isolate,
///     context: &my_global_ctx,
///     let &mut scope
/// );
/// ```
#[macro_export]
macro_rules! scope_with_context {
    (isolate: $isolate:expr, let &mut $scope:ident, let $context:ident) => {
        let $scope = std::pin::pin!(v8::HandleScope::new($isolate));
        let $scope = &mut $scope.init();

        let $context = v8::Context::new($scope, Default::default());
        let $scope = &mut v8::ContextScope::new($scope, $context);
    };

    (isolate: $isolate:expr, let $scope:ident, let $context:ident) => {
        let $scope = std::pin::pin!(v8::HandleScope::new($isolate));
        let $scope = &mut $scope.init();

        let $context = v8::Context::new($scope, Default::default());
        let $scope = v8::ContextScope::new($scope, $context);
    };

    // Enter an existing context stored as a Global<Context>.
    // No new context is created; microtasks are queued and drained
    // in the same context across every scope entry.
    (isolate: $isolate:expr, context: $gctx:expr, let &mut $scope:ident) => {
        let $scope = std::pin::pin!(v8::HandleScope::new($isolate));
        let $scope = &mut $scope.init();
        let _ctx_local = v8::Local::new($scope, $gctx);
        let $scope = &mut v8::ContextScope::new($scope, _ctx_local);
    };
}

/// Create a try-catch scope for error handling.
#[macro_export]
macro_rules! try_catch {
    (scope: $scope:ident, let $exception:ident) => {
        let $exception = std::pin::pin!(v8::TryCatch::new($scope));
        let $exception = &mut $exception.init();
    };
}
