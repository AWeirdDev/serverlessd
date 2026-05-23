---
title: "Worker Config"
weight: 3
---

Worker configuration files (usually `wrangler.toml`) allow you to specify what each worker is capable of doing, what they do, and more.

serverlessd partially derives the schema from [Cloudflare's Wrangler (TOML)](https://developers.cloudflare.com/workers/wrangler/configuration/)
so it's easy to get started with.

> [!WARNING]
> This is **currently** only available in `serverlessd one` for running one singular worker only.

For example, a worker called "hello" with the entrypoint `main.js`:

```toml {filename="wrangler.toml"}
name = "hello"
main = "./main.js"
```

## Specifying bindings
You can specify **bindings** to extend what your worker can do.
For example, if you have a binding type called `kv` and you'd like to bind it to the name `MY_KV_DATABASE`,
you can add this line:

```toml
[kv]
binding = "MY_KV_DATABASE"
```

serverlessd then knows that `MY_KV_DATABASE` binds to a key-value store.
