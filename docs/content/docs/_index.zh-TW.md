---
date: '2026-05-16T23:15:57+08:00'
title: ''
cascade:
  type: docs
---

# serverlessd

一個輕量級的 Serverless Worker Runtime，為所有人而打造。中文聽起來超憨。

簡單來說，**Serverless Worker** 是一種快速、精簡的執行環境，可以在需要時動態執行小型、事件驅動的函式，而不需要使用者或開發者自行管理伺服器。

而 **Serverless Runtime** 的工作，就是讓這些 Worker 能夠安全、穩定地運作。

這個專案的目標，是打造一個簡單、可靠、且具備擴充性的 Serverless Runtime。

## 架構

整體架構其實滿直觀的。在一個 Serverless Runtime 裡，可以同時存在多個 Worker，而每個 Worker 都由兩條執行緒組成：

- **Monitor Thread**：負責監控 Worker，檢查是否超過執行時間限制。
- **Worker Thread**：單執行緒的非同步 Runtime，實際負責執行 Worker。

由於只有一個 Worker Thread，因此如果採用建議的 timeout 設定，通常建議每個 Pod 放大約 2～3 個 Worker。

另外，Runtime、Pods 與 Workers 之間採用訊息傳遞溝通，因此整體設計幾乎是 lock-free 的。

```txt
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

## 安裝

安裝前，建議先閱讀透過 `curl` 從 [svld.aweird.me/install.sh](https://svld.aweird.me/install.sh) 下載的安裝腳本內容。

之後可以直接執行：

```sh
curl -fsSL https://svld.aweird.me/install.sh | sh
```

如果你還是對安全性有疑慮，也可以改從 [GitHub Releases](https://github.com/AWeirdDev/serverlessd/releases) 頁面手動下載。
只是要多打幾個字而已，有點懶。

## 使用工具

這個專案是在[這台電腦](https://www.apple.com/macbook-air/)上完成的。

另外也用了憨仔 Claude，但只有在一些我懶得處理的地方：

- `fetch()` 的實作
- 超憨 `ReadableStream` 實踐
- JavaScript 模組實例化

除此之外，整個過程都有真人參與。以防你不知道我是人。

## 社展

2026 建北電資聯合社展 [latent](https://exhibit.ckefgisc.org/)

- 活動時間：05/31 （日）10:30~17:00
- 活動地點：建中夢紅樓一樓

欸幹有沒有人 5/30 要去看 Backrooms 揪揪揪揪
