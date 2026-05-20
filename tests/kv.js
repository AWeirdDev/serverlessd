export default {
  async fetch() {
    return new Response(JSON.stringify(await env.KV.get("hello")).toString(), {
      status: 200,
    });
  },
};
z;
