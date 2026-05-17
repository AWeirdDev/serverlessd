---
title: "serverlessd run"
cascade:
  type: docs
---

Runs the full serverless runtime.

The amount of memory needed is determined by the `pods` and `pods-per-worker` options.

**Example**:

```sh
serverlessd run --port 8080 --host 0.0.0.0 --pods 10 --workers-per-pod 2
```

You get 10 \* 2 = 20, a maximum of 20 workers.

## `--debug`
Whether to enable debug mode and show verbose logs.

## `--port <PORT>`
Assigns the port to use. Defaults to `3000`.

## `--host <HOST>`
Assigns the host to use. Defaults to `127.0.0.1`.

## `--pods <PODS>`
The number of pods (threads) for serverless execution.
You can put as many threads as your hardware allows.

## `--workers-per-pod <WORKERS_PER_POD>`
The number of workers per pod (thread) for serverless execution.

It's recommended to use a lower amount (about 2~3) so the delay between switching 
await points (which is usually caused by CPU tasks) from worker to worker can be reduced.
