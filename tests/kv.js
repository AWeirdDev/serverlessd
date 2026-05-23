export default {
  async fetch(_request, env) {
    await env.KV.put("hello", "world");
    let hello = await env.KV.get("hello");
    return new Response(JSON.stringify(hello), {
      status: 200,
    });
  },
};
