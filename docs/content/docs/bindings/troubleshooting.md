---
title: "Troubleshooting"
---
Below are some common errors.

> [!NOTE]
> You should turn on `--debug` to troubleshoot. Messages only show up when debug mode is active.

## Address already in use
If you see a message like this in debug mode:

```rs
ERROR serverlessd: failed to start bindings server, error: CreationError(Os { code: 48, kind: AddrInUse, message: "Address already in use" })
```

It means one of two things:
1. There's already a (serverlessd) server using that socket. Shut it off.
2. The most recent serverlessd server crashed and it did not clean up the socket.


### Solution
Try removing the socket file directly:

```sh
rm .serverlessd/bindings.sock
```

If you see errors like the file is in use, turn off any running server that's currently using this socket.
