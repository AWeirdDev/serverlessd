export default {
  async fetch(_request, env) {
    let hello = await env.KV.get("hello");
    return new Response(JSON.stringify(hello), {
      status: 200,
    });
  },
};
