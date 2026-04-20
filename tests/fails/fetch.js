function use_cpu(times) {
  for (let i = 0; i < times; i++) {
    Math.sqrt(Math.random());
  }
}

let logs = "";

console.log = (msg) => (logs += msg + "\n");

async function main() {
  console.log(`worker start at ${new Date().toISOString()}`);
  let x = 0;
  let all = 0;
  for (let i = 0; i < 40; i++) {
    x++;
    const start = Date.now();
    console.log(`start use cpu at ${new Date(start).toISOString()}`);
    use_cpu(1e7 * 1);
    const elapsed = Date.now() - start;
    console.log(
      `stop use cpu at ${new Date().toISOString()}, used ${elapsed}ms, x=${x}`,
    );
    all += elapsed;
    await fetch("http://localhost:3000/delay/0");
  }
  console.log(
    `worker stop at ${new Date().toISOString()}, await done, x=${x}, all=${all}ms`,
  );

  return logs;
}

export default {
  fetch: main,
};

// main();
