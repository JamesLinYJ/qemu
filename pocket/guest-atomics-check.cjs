// SPDX-License-Identifier: MIT
const assert = require('node:assert/strict');
const { Worker, isMainThread, workerData } = require('node:worker_threads');

if (isMainThread) {
  const memory = new SharedArrayBuffer(4);
  const jobs = Array.from({ length: 2 }, () => new Promise((resolve, reject) => {
    const worker = new Worker(__filename, { workerData: memory });
    worker.once('error', reject);
    worker.once('exit', code => code === 0 ? resolve() : reject(new Error(`worker exit ${code}`)));
  }));
  Promise.all(jobs).then(() => {
    assert.equal(Atomics.load(new Int32Array(memory), 0), 20000);
    console.log('shared-memory-atomics-ok');
  }).catch(error => { console.error(error); process.exitCode = 1; });
} else {
  const values = new Int32Array(workerData);
  for (let i = 0; i < 10000; i++) Atomics.add(values, 0, 1);
}
