export default {
  async fetch(_request, env) {
    await env.KV.put("hello", "world");
    return new Response(await env.KV.get("hello"), {
      status: 200,
    });
  },
};
