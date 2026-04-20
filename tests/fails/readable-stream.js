const stream = new ReadableStream({
  start(controller) {
    const chunks = ["Hello", ", ", "world", "!"];
    let index = 0;

    while (index < chunks.length) {
      controller.enqueue(chunks[index++]);
    }

    controller.close();
  },
});

const reader = stream.getReader();

async function read() {
  let log = "";
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    log += "Chunk:" + value + "\n";
  }

  return log;
}

export default {
  async fetch() {
    return await read();
  },
};
