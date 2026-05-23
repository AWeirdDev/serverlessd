export default {
  async fetch(_request, env) {
    await fetch("https://google.com");

    return new Response(JSON.stringify("yes"), {
      status: 200,
    });
  },
};
