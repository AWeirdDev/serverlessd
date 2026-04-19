function use_cpu(times) {
  for (let i = 0; i < times; i++) {
    Math.sqrt(Math.random());
  }
}

let logs = "";
console.log = (msg) => (logs += msg + "\n");

export default {
  async fetch() {
    console.log(`worker start at ${new Date().toISOString()}`);

    fetch("http://127.0.0.1:3000").then((e) => {
      let start = Date.now();
      console.log("start use cpu at " + new Date(start).toISOString());
      use_cpu(1e9);
      console.log(
        `stop use cpu at ${new Date().toISOString()}, used ${Date.now() - start}ms`,
      );
    });

    let start = Date.now();

    console.log("start await at " + new Date(start).toISOString());
    await fetch("https://httpbin.org/delay/1");
    console.log(
      `stop await at ${new Date().toISOString()}, await ${Date.now() - start}ms`,
    );
    return logs;
  },
};
