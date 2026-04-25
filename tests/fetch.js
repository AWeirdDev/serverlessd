export default {
  async fetch() {
    let res = await fetch("https://httpbin.org/anything");
    let result = await res.json();
    return new Response(JSON.stringify(result), {
      status: 200,
    });
  },
};
