export default {
  async fetch(_request, env) {
    return new Response(JSON.stringify(Object.keys(env.KV)), {
      status: 200,
    });
  },
};
