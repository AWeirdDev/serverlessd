---
title: "Custom Bindings"
---

You can write custom bindings by using [the bindings protocol](./protocol).

## Rust
In Rust, there's already an official client library you can use. You can add it to your dependencies.

```toml {filename="Cargo.toml"}
[dependencies]
# The client library
svld-ipc-client = { git = "https://github.com/AWeirdDev/serverlessd" }

# JSON support
serde_json = "1"
```

First, connect to the socket and perform a handshake.

```rs
use std::path::PathBuf;

let mut client = svld_ipc_client::connect()
    .binding_type("my_binding".into()) // your binding type
    .path(PathBuf::new())
    .call()
    .await?
    .perform_handshake()
    .await?;
```

Then, within a loop, receive server-side messages and reply:

```rs
use svld_ipc_client::ServerMessage;

while let Ok(ServerMessage {
    id,
    function_name,
    worker_name,
    args,
}) = client.recv_message::<Vec<serde_json::Value>>().await {
    // echo back
    client
        .send_ok(
            id, 
            format!(
                "{} called {}, with args: {:?}", 
                worker_name, 
                function_name, 
                args
            )
        )
        .await?;
}
```

> [!TIP] ^C Safety
> You can make the program safely exit on ^C.
> 
> ```rs
> let ctrl_c = tokio::signal::ctrl_c();
> tokio::pin!(ctrl_c);
> 
> loop {
>   let server_message = tokio::select! {
>     _ = &mut ctrl_c => {
>         break;
>     }
>     msg = client.recv_message::<Vec<serde_json::IValue>>() => msg
>   };
> 
>  let ServerMessage {
>     id,
>     function_name,
>     worker_name,
>     args,
>  } = match server_message {
>     Ok(t) => t,
>     Err(err) => {
>       eprintln!("error while receiving message: {err:?}");
>       break;
>     }
>   };
> 
>   // ... other tasks
> }
> ```

## Testing your bindings
After you're done making your bindings, test them out!

Add bindings your configuration file:

```toml {filename="wrangler.toml"}
[my-binding-1]
binding = "MY_BINDING_ONE"

[my-binding-2]
binding = "MY_BINDING_TWO"
```

And the JavaScript worker script:

```js {filename="main.js"}
export default {
  async fetch(request, env) {
    await env.MY_BINDING_ONE.foo();
    await env.MY_BINDING_TWO.bar(123, 4.56);

    return "foo";
  }
}
```

```sh
serverlessd one \
    ./wrangler.toml \
    --bindings my-binding-1 \
    --bindings my-binding-2
```

The `--bindings` flag specifies what bindings the worker has access to.

After starting a serverlessd instance, you can start your bindings to make them
connect to the IPC server.
