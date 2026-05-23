---
title: "Protocol"
---

The bindings protocol enables developers to bring their custom bindings to the runtime.
Upon running serverlessd, it creates an [IPC (interprocess communication)](https://en.wikipedia.org/wiki/Inter-process_communication) server which we (clients) can connect.

Additionally, binding calls are just like function calls.

Generally speaking, there are 3 stages of a binding client after you've successfully
connected to the bindings server:

- **Connecting** (Uninitialized)
- **Handshake** (Initializing)
- **Main loop** (Initialized)

## Connecting
First, ensure that a serverlessd process is running. In the same parent directory the process is
running, there should be a `.serverlessd/` directory containing a file called `bindings.sock`.

```{filename="Path"}
.serverlessd/bindings.sock
```

Connect to the IPC socket and continue with handshaking.

## Handshaking
After you've successfully connected to the server, you'll have to identify yourself.
Namely, specify what type of binding you are. If the same type of binding has connected
to the server before, you'll get rejected.

**We (clients) send**:

```c
[uint32_t] [...payload]
```

What we send is:
- 4 bytes of unsigned integer (`u32`) which specifies the length of the incoming payload.
- Payload data containing the type of your binding in UTF-8 bytes.

## Main loop
The main loop consists of receiving and replying.

After the server accepts your handshake, you can start receiving from the server for incoming
function call requests and reply to them.

### Receiving

**We (clients) receive**:

```c
[uint32_t] [uint32_t] [...payload]
```

What we receive is:
- 4 bytes of unsigned integer (`u32`) which specifies the ID of the request. This is used for replying.
- 4 bytes of unsigned integer (`u32`) which specifies the length of the incoming payload.
- Payload data in JSON encoded in UTF-8 bytes.

The payload JSON format is as follows:

```jsonc
{
  "func": "some_func", // name of the function to call
  "worker": "worker_a", // name of the worker triggering this
  "args": [ // arguments in an array
    "arg",
    100,
    true,
    1.234
  ],
}
```

### Replying
After we're done processing the worker's request, we can reply to it.

**We (clients) send**:

```c
[uint32_t] [uint32_t] [...payload]
```

What we send is:
- 4 bytes of unsigned integer (`u32`) which specifies the ID of the request. This is used for replying.
- 4 bytes of unsigned integer (`u32`) which specifies the length of the incoming payload.
- Payload data in JSON encoded in UTF-8 bytes.

The payload JSON format is as follows.

**Success format**: Send this when the operation is successful.

```jsonc
{
  "data": ... // any JSON data
}
```


**Failure format**: Send this when the operation failed.

```jsonc
{
  "error": "This operation failed!" // error message (string)
}
```
