---
date: '2026-05-16T23:19:24+08:00'
draft: false
title: 'Worker 腳本'
cascade:
  type: docs
---

Worker Script 是執行於受控 JavaScript 環境中的腳本，可以透過 HTTP 事件（例如 `GET`、`POST` 等）或 CRON 排程觸發執行。

Worker Script 必須符合以下要求：

- 不可存在可能失敗的導入 (imports)
- 必須導出 (export) 一個預設物件 (default object)
- 導出的預設物件內的事件處理函式必須是非同步 (async)

只要符合 **以下任一條件**，你的 Worker 就會被直接終止：

- 不符合上述任一要求
- 在 await 之間累積的執行時間（也就是偏向 CPU-bound 的工作）超過 10ms
- Wall time（整體 Worker 執行時間）超過 10 秒

因此，Worker 通常應該設計成輕量、不耗時、且不吃 CPU 的任務。

---

## Worker 範例

下面是一個最基本的 Worker，會對所有 HTTP Request 回傳 `"Hello, world!"`。

```js
export default {
  // 當有 request 進來時會呼叫這個函式
  async fetch(request, env) {
    // 你可以做一些稍微重一點的運算，
    // 只要累積時間低於 10ms 即可
    for (let i = 0; i < 100; i++) {
      Math.cos(Math.random() * Math.PI);
    }

    return new Response("Hello, world!", { status: 200 });
  }
};
```

---

## Web API 相容性

目前支援情況如下：

- `fetch()`：大部分基本功能可使用
- `ReadableStream`：大部分基本功能可使用
- `Response`：大部分基本功能可使用
- `Request`：目前尚未提供／無法存取
