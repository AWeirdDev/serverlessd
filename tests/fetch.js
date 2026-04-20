export default {
  async fetch() {
    let res = await fetch("https://httpbin.org/anything");
    return "no time out please";
  },
};
