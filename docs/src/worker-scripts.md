# Worker scripts
Worker scripts are similar to Cloudflare Worker's. For example, a site that always returns "Hello, world":

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
