export default {
  async fetch(request) {
    return new Response(request.url, {
      status: 200,
      headers: {
        "Content-Type": "text/html",
      },
    });
  },
};
