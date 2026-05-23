---
title: "KV"
weight: 1
---

A key-value store.

You can download it via [GitHub Releases](https://github.com/AWeirdDev/serverlessd/releases).
It should be named `binding-kv-{TARGET}`, where `{TARGET}` is the target architecture and operating system.

Start the server once a serverlessd instance is started:

```sh
binding-kv -p path/to/.serverlessd
```

## Get
Gets a value from the key. If the item does not exist, `null` is returned.

Throws an error if this operation fails.

```ts
declare async function get(key: string): string | null;
```

## Put
Puts (sets) a value to the key.

Throws an error if this operation fails.

```ts
declare async function put(key: string, value: string);
```

## Delete
Deletes a value using the key.

Throws an error if this operation fails.

```ts
declare async function delete(key: string);
```

## List
Lists all keys. Returns an array of objects, containg the field `name` which describes
the name of the key.

This operation does not fail. Keys that canot be accessed (generally errored out) are skiped.

```ts
interface Key {
  name: string,
}

declare async function list(): Key[];
```
