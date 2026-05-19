---
title: "serverlessd run"
cascade:
  type: docs
---

執行完整的無伺服器執行環境。
所需記憶體大小由 `pods` 和 `pods-per-worker` 參數決定。

**範例**：
```sh
serverlessd run --port 8080 --host 0.0.0.0 --pods 10 --workers-per-pod 2
```

10 \* 2 = 20，最多可有 20 個 worker。

## `--debug`
是否啟用除錯模式並顯示詳細日誌。

## `--port <PORT>`
指定使用的連接埠。預設為 `3000`。

## `--host <HOST>`
指定使用的主機位址。預設為 `127.0.0.1`。

## `--pods <PODS>`
無伺服器執行時的 pod（執行緒）數量。可依照硬體支援的上限自由設定執行緒數。

## `--workers-per-pod <WORKERS_PER_POD>`
每個 pod（執行緒）的 worker 數量。
建議設低一點（大約 2～3 個），這樣可以減少 worker 之間切換 await 點的延遲（通常是 CPU 密集任務造成的）。
