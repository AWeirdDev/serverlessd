<div align="center">

<h1>serverlessd<br /><sup>(serverless runtime)</sup></h1>
![github build](https://img.shields.io/github/actions/workflow/status/AWeirdDev/serverlessd/ci.yml)

</div>

<br />

Serverless workers management architecture. **Work in progress**.

I'm working on the "automatic scaling" part now, just to be clear.

You can use this for:

- Getting a local serverless runtime up and running
- Custom LLM toolkit for fast execution

To start:

```sh
$ serverlessd run --n-pods 10 --n-workers-per-pod 2
=====> server started at http://127.0.0.1:3000
```

***

## 介紹
一個基於 V8 的 Serverless Runtime，目標相容 Cloudflare Workers，但加了一些自訂義功能。

這個專案讓你可以在不需要管理伺服器的情況下執行 JavaScript。

基本上寫一段程式：
- 有請求進來時就執行
- 自動做資源擴展
- 用完就結束

概念類似 [Cloudflare Workers](https://developers.cloudflare.com/workers)。

好，其實基本上我也不知道我在做啥。

***

[社展](https://www.instagram.com/ckefgisc_latent_2026)
