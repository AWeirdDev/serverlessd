function use_cpu(times) {
  for (let i = 0; i < times; i++) {
    Math.sqrt(Math.random());
  }
}

let logs = "";

console.log = (msg) => (logs += msg + "\n");

let start = Date.now();
console.log("start use cpu at " + new Date(start).toISOString());
use_cpu(1e9 * 1);
console.log(
  `stop use cpu at ${new Date().toISOString()}, used ${Date.now() - start}ms`,
);

async function main() {
  console.log(`worker start at ${new Date().toISOString()}`);
  return logs;
}

export default {
  fetch: main,
};
