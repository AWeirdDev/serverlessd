export default {
  async fetch(request, env) {
    return new Response(typeof env, {
      status: 200,
      headers: {
        "Content-Type": "text/html",
      },
    });
  },
};
