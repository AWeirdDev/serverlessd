export default {
  async fetch() {
    env.KV.get("hello");
    return new Response("hello, world!", { status: 200 });
  },
};
