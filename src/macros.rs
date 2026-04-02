/// Create a scope with context.
#[macro_export]
macro_rules! scope_with_context {
    (isolate: $isolate:expr, let $scope:ident, let $context:ident) => {
        let $scope = std::pin::pin!(v8::HandleScope::new($isolate));
        let $scope = &mut $scope.init();

        let $context = v8::Context::new($scope, Default::default());
        let $scope = &mut v8::ContextScope::new($scope, $context);
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
