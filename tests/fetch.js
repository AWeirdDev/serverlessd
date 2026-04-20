export default {
  async fetch() {
    let res = await fetch("https://httpbin.org/anything");
    let result = await res.json();
    return JSON.stringify(result);
  },
};
