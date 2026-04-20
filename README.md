<div align="center">

<h1>serverlessd<br /><sup>(serverless runtime)</sup></h1>

![github build](https://img.shields.io/github/actions/workflow/status/AWeirdDev/serverlessd/ci.yml)

</div>

<br />

Serverless workers management architecture. **Work in progress**.

You can use this for:

- Getting a local serverless runtime up and running
- Custom LLM toolkit for fast execution
- Setting up a lightweight container for running quick scripts

## Installation
You can install this via an install script ([is.gd/serverlessd](https://is.gd/serverlessd); you can inspect the code), if you'd like:

```sh
# inspect the code first
curl -fsSL https://is.gd/serverlessd

# ...then install
curl -fsSL https://is.gd/serverlessd | sh
```

<details>

<summary>Safer alternative</summary>

If you still have safety concerns, a safer alternative would be to download from the [Releases](https://github.com/AWeirdDev/serverlessd/releases).
It's just that you'd have to type a bit more. And I just hate typing.

</details>

## Getting started
To start:

```sh
$ serverlessd run --pods 10 --workers-per-pod 2
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
