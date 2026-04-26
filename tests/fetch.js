export default {
  async fetch() {
    env.KV.put("money", "ties");
    return new Response(env.KV.get("money"), {
      status: 200,
      headers: {
        "Content-Type": "text/html",
      },
    });
  },
};
