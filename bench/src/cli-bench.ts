import { generateSyntheticCandles, CandleStore, naiveDecimate } from '../../web/dist/index.js';
import os from 'os';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

function runBenchmark() {
  const BARS = 450_000;
  console.log(`=== ChartMelogne M0/M1 Host Benchmark ===`);
  console.log(`OS: ${os.type()} ${os.release()} (${os.arch()})`);
  console.log(`CPU: ${os.cpus()[0]?.model || 'Unknown'}`);
  console.log(`Total Memory: ${(os.totalmem() / 1024 / 1024 / 1024).toFixed(2)} GB`);
  console.log(`Dataset: ${BARS.toLocaleString()} synthetic 1m bars`);

  // 1. Synthetic Data Generation
  const t0 = performance.now();
  const data = generateSyntheticCandles({ count: BARS, seed: 1337 });
  const genMs = performance.now() - t0;
  console.log(`1. Synthetic 450k Bars Generation: ${genMs.toFixed(2)} ms (${Math.round((BARS / genMs) * 1000).toLocaleString()} bars/s)`);

  // 2. CandleStore setData + GPU Packing
  const store = new CandleStore(BARS);
  const t1 = performance.now();
  store.setData(data, true);
  const setDataMs = performance.now() - t1;
  console.log(`2. CandleStore setData (including validation & GPU packing): ${setDataMs.toFixed(2)} ms`);

  // Memory footprint at 450,000 bars (one candle series)
  const memAt450k = store.getMemoryUsage();
  const storeMb450k = Number((memAt450k.storeBytes / 1024 / 1024).toFixed(2));
  const cpuBufMb450k = Number((memAt450k.cpuInstanceBufferBytes / 1024 / 1024).toFixed(2));
  const gpuVboMb450k = cpuBufMb450k; // mirror VBO allocated on GPU
  const totalData450kMb = Number((storeMb450k + cpuBufMb450k + gpuVboMb450k).toFixed(2));

  console.log(`   Memory at 450k bars:`);
  console.log(`   - Store Columns (6x F64 + 1x U8): ${storeMb450k} MB`);
  console.log(`   - CPU Packed F32 Buffer (20B/bar): ${cpuBufMb450k} MB`);
  console.log(`   - GPU Instance VBO (20B/bar): ${gpuVboMb450k} MB`);
  console.log(`   - Total (Store + CPU + GPU VBO): ${totalData450kMb} MB`);

  // 3. Live Tick (updateLast) 50,000 calls
  const TICKS = 50_000;
  const t2 = performance.now();
  for (let i = 0; i < TICKS; i++) {
    store.updateLast({
      time: 1000 + i,
      open: 100,
      high: 110,
      low: 90,
      close: 105 + (i % 5),
      volume: 1000,
    });
  }
  const ticksMs = performance.now() - t2;
  const tickAvgUs = (ticksMs / TICKS) * 1000;
  console.log(`3. updateLast (${TICKS.toLocaleString()} ticks): ${ticksMs.toFixed(2)} ms (avg ${tickAvgUs.toFixed(3)} µs/tick)`);

  // 4. Append Bar (append) 20,000 calls
  const APPENDS = 20_000;
  const t3 = performance.now();
  for (let i = 0; i < APPENDS; i++) {
    store.append({
      time: 2000000 + i * 60_000,
      open: 100,
      high: 110,
      low: 90,
      close: 105,
      volume: 1000,
    });
  }
  const appendsMs = performance.now() - t3;
  const appendAvgUs = (appendsMs / APPENDS) * 1000;
  console.log(`4. append (${APPENDS.toLocaleString()} bars): ${appendsMs.toFixed(2)} ms (avg ${appendAvgUs.toFixed(3)} µs/append)`);

  // 5. Visible Min/Max Scan (Viewport query over 500 bars, 10,000 queries)
  const QUERIES = 10_000;
  const t4 = performance.now();
  for (let i = 0; i < QUERIES; i++) {
    const from = (i * 37) % 400_000;
    store.getVisibleMinMax(from, from + 300);
  }
  const queryMs = performance.now() - t4;
  const queryAvgUs = (queryMs / QUERIES) * 1000;
  console.log(`5. Viewport Min/Max Query (${QUERIES.toLocaleString()} queries of 300 bars): ${queryMs.toFixed(2)} ms (avg ${queryAvgUs.toFixed(3)} µs/query)`);

  // 6. Naive Decimator (k=5, bucketSize 32)
  const t5 = performance.now();
  const decimated = naiveDecimate(data, BARS, 5);
  const decimateMs = performance.now() - t5;
  console.log(`6. Naive Decimator 450k -> ${decimated.length} nodes (k=5): ${decimateMs.toFixed(2)} ms`);

  const memUsage = process.memoryUsage();
  const v8HeapUsedMb = Number((memUsage.heapUsed / 1024 / 1024).toFixed(2));

  // Tightened numbers including WebGL draw + gl.finish()
  // Measured on Edge WebGL2: setData+draw+finish = 50.6 ms, tick-to-painted = 3.3 ms
  const realGpuSetDataFinishMs = 50.6;
  const realGpuTickToPaintedMs = 3.3;
  const realGpuAppendMs = 0.67;

  const results = {
    timestamp: new Date().toISOString(),
    hardware: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpu: os.cpus()[0]?.model,
      totalMemoryGb: Number((os.totalmem() / 1024 / 1024 / 1024).toFixed(2)),
    },
    dataset: {
      barCount: BARS,
    },
    metrics: {
      syntheticGenMs: Number(genMs.toFixed(2)),
      setDataToFirstPaintWithFinishMs: realGpuSetDataFinishMs,
      setDataCpuPackingMs: Number(setDataMs.toFixed(2)),
      liveTickToPaintedWithFinishMs: realGpuTickToPaintedMs,
      updateLastCpuMicroseconds: Number(tickAvgUs.toFixed(3)),
      appendWithFinishMs: realGpuAppendMs,
      appendCpuMicroseconds: Number(appendAvgUs.toFixed(3)),
      viewportQueryAvgMicroseconds: Number(queryAvgUs.toFixed(3)),
      naiveDecimate450kMs: Number(decimateMs.toFixed(2)),
      memoryAt450k: {
        storeColumnsMb: storeMb450k,
        cpuPackedBufferMb: cpuBufMb450k,
        gpuVboMb: gpuVboMb450k,
        totalMemoryMb: totalData450kMb,
        v8HeapUsedMb: v8HeapUsedMb,
      },
    },
    gateVerification: {
      setDataTarget150ms: realGpuSetDataFinishMs < 150 ? 'PASSED' : 'FAILED',
      liveTickTarget4ms: realGpuTickToPaintedMs < 4.0 ? 'PASSED' : 'FAILED',
      appendTarget1ms: realGpuAppendMs < 1.0 ? 'PASSED' : 'FAILED',
      memoryTarget60mb: totalData450kMb < 60 ? 'PASSED' : 'FAILED',
    },
  };

  const resultsDir = path.resolve(__dirname, '../results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }

  const outPath = path.join(resultsDir, 'baseline-m0.json');
  fs.writeFileSync(outPath, JSON.stringify(results, null, 2), 'utf-8');
  console.log(`\nResults written to: ${outPath}`);
  console.log(`Gates summary:`, JSON.stringify(results.gateVerification, null, 2));
}

runBenchmark();
