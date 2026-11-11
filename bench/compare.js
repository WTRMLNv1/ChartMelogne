import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const resultsDir = path.resolve(__dirname, 'results');
const tsPath = path.join(resultsDir, 'baseline-m0.json');
const rustPath = path.join(resultsDir, 'baseline-rust.json');

if (!fs.existsSync(tsPath)) {
  console.error(`Missing TypeScript baseline file: ${tsPath}`);
  process.exit(1);
}

if (!fs.existsSync(rustPath)) {
  console.error(`Missing Rust baseline file: ${rustPath}`);
  console.error(`Run 'cargo run --manifest-path chartmelogne-rs/Cargo.toml -p bench --release -- headless' first.`);
  process.exit(1);
}

const tsData = JSON.parse(fs.readFileSync(tsPath, 'utf-8'));
const rustData = JSON.parse(fs.readFileSync(rustPath, 'utf-8'));

console.log('========================================================================================');
console.log('                    CHARTMELOGNE: TYPESCRIPT vs RUST BENCHMARK COMPARISON               ');
console.log('========================================================================================');
console.log(`TS Hardware:   ${tsData.hardware.cpu || tsData.hardware.platform} (${tsData.hardware.arch})`);
console.log(`Rust Hardware: ${rustData.hardware.adapterName} [${rustData.hardware.adapterBackend}] (${rustData.hardware.platform} ${rustData.hardware.arch})`);
console.log(`Dataset:       ${tsData.dataset.barCount.toLocaleString()} synthetic 1m candles`);
console.log('----------------------------------------------------------------------------------------');

const rows = [
  {
    gate: 'Memory (Store + GPU)',
    unit: 'MB',
    ts: tsData.metrics.memoryAt450k.totalMemoryMb,
    rust: rustData.metrics.memoryAt450k.totalMemoryMb,
    target: '< 60.0 MB',
    lowerIsBetter: true,
  },
  {
    gate: 'setData to First Paint',
    unit: 'ms',
    ts: tsData.metrics.setDataToFirstPaintWithFinishMs,
    rust: rustData.metrics.setDataToFirstPaintWithFinishMs,
    target: '< 150.0 ms',
    lowerIsBetter: true,
  },
  {
    gate: 'Live Tick to Painted (p95)',
    unit: 'ms',
    ts: tsData.metrics.liveTickToPaintedWithFinishMs,
    rust: rustData.metrics.liveTickToPaintedWithFinishMs,
    target: '< 4.0 ms',
    lowerIsBetter: true,
  },
  {
    gate: 'Append to Painted (p95)',
    unit: 'ms',
    ts: tsData.metrics.appendWithFinishMs,
    rust: rustData.metrics.appendWithFinishMs,
    target: '< 1.0 ms',
    lowerIsBetter: true,
  },
];

console.log(
  'Metric'.padEnd(30) +
    'TS Baseline'.padEnd(16) +
    'Rust Port'.padEnd(16) +
    'Ratio (Rust/TS)'.padEnd(20) +
    'Gate Target'.padEnd(14) +
    'Status'
);
console.log('-'.repeat(102));

let allPassed = true;

for (const row of rows) {
  const ratio = row.rust / row.ts;
  let ratioStr = `${ratio.toFixed(2)}x`;
  if (row.lowerIsBetter) {
    if (ratio < 0.95) {
      ratioStr += ` (${(1 / ratio).toFixed(1)}x faster)`;
    } else if (ratio > 1.05) {
      ratioStr += ` (${ratio.toFixed(1)}x slower)`;
    } else {
      ratioStr += ` (~parity)`;
    }
  }

  const passed =
    row.gate.includes('Memory')
      ? row.rust < 60.0
      : row.gate.includes('setData')
      ? row.rust < 150.0
      : row.gate.includes('Live Tick')
      ? row.rust < 4.0
      : row.rust < 1.0;

  if (!passed) allPassed = false;

  console.log(
    row.gate.padEnd(30) +
      `${row.ts.toFixed(2)} ${row.unit}`.padEnd(16) +
      `${row.rust.toFixed(2)} ${row.unit}`.padEnd(16) +
      ratioStr.padEnd(20) +
      row.target.padEnd(14) +
      (passed ? 'PASS' : 'FAIL')
  );
}

console.log('----------------------------------------------------------------------------------------');
console.log('\nDetailed Memory Breakdown:');
console.log(`  - Store Columns:        TS: ${tsData.metrics.memoryAt450k.storeColumnsMb} MB  | Rust: ${rustData.metrics.memoryAt450k.storeColumnsMb} MB`);
console.log(`  - GPU Instance Buffer:  TS: ${tsData.metrics.memoryAt450k.gpuVboMb} MB  | Rust: ${rustData.metrics.memoryAt450k.gpuVboMb} MB`);
console.log(`  - CPU Packed Buffer:    TS: ${tsData.metrics.memoryAt450k.cpuPackedBufferMb} MB  | Rust: ${rustData.metrics.memoryAt450k.cpuPackedBufferMb} MB`);
console.log(`  - Process Memory:       TS: ${tsData.metrics.memoryAt450k.v8HeapUsedMb || 'N/A'} MB (JS Heap) | Rust: ${rustData.metrics.memoryAt450k.processWorkingSetMb} MB (Working Set)`);

if (rustData.scenarios) {
  console.log('\nRust Headless Scenarios Performance (10s continuous at 450,000 bars):');
  for (const [name, sc] of Object.entries(rustData.scenarios)) {
    console.log(
      `  - ${name.padEnd(20)}: ${String(sc.total_frames).padStart(4)} frames, ` +
        `${sc.fps.toFixed(1).padStart(5)} fps, ` +
        `p50=${sc.p50_ms.toFixed(2)} ms, p95=${sc.p95_ms.toFixed(2)} ms, max=${sc.max_ms.toFixed(2)} ms, ` +
        `dropped=${sc.dropped_frames}`
    );
  }
}

console.log('========================================================================================');
if (allPassed) {
  console.log('ALL M0 GATES VERIFIED: RUST PORT PASSES SPECIFICATION!');
} else {
  console.error('ONE OR MORE GATES FAILED SPECIFICATION');
  process.exit(1);
}
