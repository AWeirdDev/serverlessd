use v8::{Function, Local};

use crate::{
    language::{ThrowException, throw},
    try_catch,
};

pub struct Response;

#[allow(unused)]
impl Response {
    #[inline(always)]
    #[must_use]
    pub fn builder<'s, 'i, 'k>(scope: &'k v8::PinScope<'s, 'i>) -> ResponseBuilder<'s, 'i, 'k> {
        ResponseBuilder::new(scope)
    }

    pub fn get_new_function<'s>(scope: &v8::PinScope<'s, '_>) -> Local<'s, Function> {
        let function_template = v8::FunctionTemplate::new(scope, Self::constructor);
        let function = function_template.get_function(scope).unwrap();

        function
    }

    fn constructor(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        fn inner(
            scope: &mut v8::PinScope,
            args: v8::FunctionCallbackArguments,
            mut rv: v8::ReturnValue,
        ) -> Option<()> {
            let this = args.this();

            let body = args.get(0);
            let options = {
                let arg = args.get(1);
                if arg.is_object() && !arg.is_null_or_undefined() {
                    Some(arg.cast::<v8::Object>())
                } else {
                    None
                }
            };

            this.set(scope, v8::String::new(scope, "body")?.cast(), body);

            rv.set(this.cast());

            Some(())
        }
    }

    fn json(
        scope: &mut v8::PinScope,
        args: v8::FunctionCallbackArguments,
        mut rv: v8::ReturnValue,
    ) {
        let data = args.get(0);
        let options = args.get(1);

        if data.is_null_or_undefined() && options.is_null_or_undefined() {
            rv.set(throw(
                scope,
                ThrowException::type_error(
                    "Response.json: At least 1 argument required, but only 0 passed",
                ),
            ));
            return;
        }

        let result = {
            try_catch!(scope: scope, let try_catch);

            let Some(result) = v8::json::stringify(try_catch, data) else {
                if let Some(exc) = try_catch.exception() {
                    rv.set(exc);
                }
                return;
            };

            result
        };

        let maybe_resp = Response::builder(scope).build();
        match maybe_resp {
            Some(t) => rv.set(t.cast()),
            None => {
                rv.set(throw(
                    scope,
                    ThrowException::error("unknown error occurred while building response"),
                ));
            }
        }
    }
}

pub struct ResponseBuilder<'s, 'i, 'k> {
    scope: &'k v8::PinScope<'s, 'i>,
    this: Local<'s, v8::Object>,
}

#[allow(unused)]
impl<'s, 'i, 'k> ResponseBuilder<'s, 'i, 'k> {
    /// Creates a new response builder.
    #[inline]
    #[must_use]
    pub fn new(scope: &'k v8::PinScope<'s, 'i>) -> Self {
        Self {
            scope,
            this: v8::Object::new(scope),
        }
    }

    #[must_use]
    pub fn status(self, code: u16) -> Self {
        let code_k = v8::String::new(self.scope, "status");
        if let Some(code_key) = code_k {
            self.this.set(
                self.scope,
                code_key.cast(),
                v8::Number::new(self.scope, code as f64).cast(),
            );
        }

        self
    }

    #[must_use]
    pub fn url<K: AsRef<str>>(self, url: K) -> Self {
        let url_k = v8::String::new(self.scope, "url");

        // i hate nested shits. but idk who the fuck designed this rusty
        // ass language
        if let Some(url_key) = url_k {
            if let Some(url) = v8::String::new(self.scope, url.as_ref()) {
                self.this.set(self.scope, url_key.cast(), url.cast());
            }
        }

        self
    }

    #[must_use]
    pub fn type_<K: AsRef<str>>(self, name: K) -> Self {
        let type_k = v8::String::new(self.scope, "type");

        if let Some(type_key) = type_k {
            if let Some(url) = v8::String::new(self.scope, name.as_ref()) {
                self.this.set(self.scope, type_key.cast(), url.cast());
            }
        }

        self
    }

    #[must_use]
    pub fn redirected(self, redirected: bool) -> Self {
        let redir_k = v8::String::new(self.scope, "redirected");

        if let Some(redir_key) = redir_k {
            self.this.set(
                self.scope,
                redir_key.cast(),
                v8::Boolean::new(self.scope, redirected).cast(),
            );
        }

        self
    }

    #[must_use]
    pub fn build(self) -> Option<Local<'s, v8::Object>> {
        // derived values
        // .ok
        {
            let status = self
                .this
                .get(self.scope, v8::String::new(self.scope, "status")?.cast())?;

            self.this
                .set(self.scope, v8::String::new(self.scope, "ok")?.cast(), {
                    if status.is_null_or_undefined() {
                        v8::Number::new(self.scope, 200 as f64).cast()
                    } else {
                        status
                    }
                });
        }

        // state values
        {
            self.this.set(
                self.scope,
                v8::String::new(self.scope, "bodyUsed")?.cast(),
                v8::Boolean::new(self.scope, false).cast(),
            );
        }

        Some(self.this)
    }
}
