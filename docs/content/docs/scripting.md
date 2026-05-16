---
date: '2026-05-16T23:19:24+08:00'
draft: true
title: 'Scripting'
cascade:
  type: docs
---

# Worker scripts
Worker scripts are scripts that run in a controlled JavaScript environment which can be triggered via HTTP events (such as `GET`, `POST`, etc.) or CRON schedules.

Worker scripts should satisfy the following requirements:

- No fallible imports
- A default object must be exported
- Within the object, the dispatch functions should be asynchronous

If **ANY** of the following is met, your worker gets terminated:

- Any of the above requirements is not met
- Accumulated time for tasks between await points (in other words, tasks that are likely CPU-bound) is more than 10ms
- Wall time (the whole execution time of the worker) is more than 10s

Generally, workers should be lightweight tasks that don't take too much time or CPU.

## Example worker
Below is a simple worker that says "Hello, world!" to every HTTP request that comes in.

```js
export default {
  // This gets called when a request comes in
  async fetch() {
    // You can do some heavy calculation, as long as the 
    // accumulated time is <10ms
    for (let i = 0; i < 100; i++) {
      Math.cos(Math.random() * Math.PI);
    }
    
    return new Response("Hello, world!", { status: 200 });
  }
};
```

## Web API compatibility

- `fetch()`: General functionalities available
- `ReadableStream`: General functionalities available
- `Response`: General functionalities available
- `Request`: Not available/accessible
