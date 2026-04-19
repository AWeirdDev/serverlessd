export default {
  async fetch() {
    let resp = Response.json({ hello: "world" });
    let reader = resp.body.getReader();
    await reader.read();
    return "ok";
  },
};
