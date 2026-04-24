# serverlessd

A lightweight serverless worker runtime.

A **serverless worker** is a fast, minimal environment that runs small event-driven functions on demand, without requiring users/developers to manage servers.

## Architecture
Overall, the architecture is self-explanatory. Within a Serverless Runtime, there can be numerous workers, and each worker consists of two threads: the **Monitor Thread** and the **Worker Thread**.

- **Monitor thread**: For monitoring workers, checking whether they've exceeded the time limit.
- **Worker thread**: An single-threaded asynchornous runtime for running workers.

Since there's only one worker thread, it's recommended to put about 2-3 workers per pod, if following the recommended timeout settings.

Serverless runtime, pods, and workers communicate via message passing, making it near lock-free.

```python
┌─────────────────────────────────────────────────────────────────────┐
│                        Serverless Runtime                           │
│                                                                     │
│  ┌──────────────────────────┐   ┌──────────────────────────┐        │
│  │          Pod 0           │   │          Pod 1           │  ...   │
│  │                          │   │                          │  more  │
│  │  ┌────────────────────┐  │   │  ┌────────────────────┐  │  pods  │
│  │  │   Monitor Thread   │  │   │  │   Monitor Thread   │  │        │
│  │  └────────────────────┘  │   │  └────────────────────┘  │        │
│  │                          │   │                          │        │
│  │  ┌────────────────────┐  │   │  ┌────────────────────┐  │        │
│  │  │   Worker Thread    │  │   │  │   Worker Thread    │  │        │
│  │  │                    │  │   │  │                    │  │        │
│  │  │  ┌──────────────┐  │  │   │  │  ┌──────────────┐  │  │        │
│  │  │  │   Worker 0   │  │  │   │  │  │   Worker 0   │  │  │        │
│  │  │  ├──────────────┤  │  │   │  │  ├──────────────┤  │  │        │
│  │  │  │   Worker 1   │  │  │   │  │  │   Worker 1   │  │  │        │
│  │  │  ├──────────────┤  │  │   │  │  ├──────────────┤  │  │        │
│  │  │  │   Worker 2   │  │  │   │  │  │   Worker 2   │  │  │        │
│  │  │  └──────────────┘  │  │   │  │  └──────────────┘  │  │        │
│  │  └────────────────────┘  │   │  └────────────────────┘  │        │
│  └──────────────────────────┘   └──────────────────────────┘        │
└─────────────────────────────────────────────────────────────────────┘
```

## Installation
Before installation, make sure to read the install script hosted at [is.gd/serverlessd](https://is.gd/serverlessd) via `curl`.
Then you can install:

```sh
curl -fsSL https://is.gd/serverlessd | sh
```

If you still have safety concerns, a safer alternative is to download from the [Releases](https://github.com/AWeirdDev/serverlessd/releases).
It's just that you'd have to type a bit more. And I just hate typing.

## Tools
This project was made possible with [this computer](https://www.apple.com/macbook-air/).

[Claude](https://claude.ai) was used, but only in these parts, which I'm too lazy to deal with:
- The `fetch()` implementation
- Fucking `ReadableStream` implementation
- JavaScript module instantiation, like a few lines

Additionally, a human was always in the loop, because they couldn't code or think at all.
