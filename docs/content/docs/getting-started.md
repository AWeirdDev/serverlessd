---
title: "Getting Started"
weight: 1
---

> [!INFO]
> Be sure to install serverlessd first!
View the quick installation guide [here](../).

Let's **create our first worker** and **run it locally** without deploying.

First, create a file called `wrangler.toml`:

```toml {filename="wrangler.toml"}
name = "hello" # Name of the worker
main = "./main.js" # The entrypoint of your worker
```

`wrangler.toml` serves as a [configuration file](./config) which specifies what the worker identifies as
and what it can do. To get things up and running quickly, we'll just specify it's a worker called "hello"
with the worker script located at `./main.js`.

Then you can write your [worker script in JavaScript](./scripting) in the same directory as `wrangler.toml`:

```js {filename="main.js"}
export default {
  async fetch() {
    return "Hello, World!"
  }
}
```

For this worker, all it does is return some text (`Hello, World!`) with the default status code 200 OK.
We can run it locally to see if it works as expected.

We can use the `serverlessd` CLI to start a "one" instance, which is designed for running one singular
worker for testing.

```sh
serverlessd one ./wrangler.toml
```

> [!NOTE]
> By default, the server runs at port `3000` & host `127.0.0.1`.
> You can specify which port/host to run with `--port <PORT> --host <HOST>`.

Open up [http://localhost:3000/worker/one](http://localhost:3000/worker/one) (or the port you specified).
You should see a piece of text appearing on the page:

```
Hello, World!
```

You're good to go! Learn more about serverlessd and how you can leverage serverless runtimes by exploring
the documentation or just trying things yourself. Have fun!
