import {
  createChart,
  generateSyntheticCandles,
  darkTheme,
  lightTheme,
  type Theme,
} from 'chartmln';
import { MetricsTracker, getWebGLInfo, type BenchmarkReport, type MemoryBreakdown } from './metrics.js';
import { BenchmarkRunner } from './scenarios.js';

const BAR_COUNT = 450_000;

const container = document.getElementById('chart-container') as HTMLElement;
const statBars = document.getElementById('stat-bars') as HTMLElement;
const statVisible = document.getElementById('stat-visible') as HTMLElement;
const statFps = document.getElementById('stat-fps') as HTMLElement;
const statP95 = document.getElementById('stat-p95') as HTMLElement;
const statUpload = document.getElementById('stat-upload') as HTMLElement;
const statTick = document.getElementById('stat-tick') as HTMLElement;
const statHeap = document.getElementById('stat-heap') as HTMLElement;

const benchOverlay = document.getElementById('bench-overlay') as HTMLElement;
const benchScenarioName = document.getElementById('bench-scenario-name') as HTMLElement;
const benchTimer = document.getElementById('bench-timer') as HTMLElement;
const benchProgress = document.getElementById('bench-progress') as HTMLElement;
const benchOutput = document.getElementById('bench-output') as HTMLElement;

const btnRunAll = document.getElementById('btn-run-all') as HTMLButtonElement;
const btnPan = document.getElementById('btn-pan') as HTMLButtonElement;
const btnZoom = document.getElementById('btn-zoom') as HTMLButtonElement;
const btnTick = document.getElementById('btn-tick') as HTMLButtonElement;
const btnAppend = document.getElementById('btn-append') as HTMLButtonElement;
const btnResize = document.getElementById('btn-resize') as HTMLButtonElement;
const btnExport = document.getElementById('btn-export') as HTMLButtonElement;
const btnTheme = document.getElementById('btn-theme') as HTMLButtonElement;

let currentTheme: Theme = darkTheme;

// Create Chart
const chart = createChart(container, {
  theme: currentTheme,
});

const candles = chart.addCandles({ capacity: 500_000 });
const metricsTracker = new MetricsTracker();
const runner = new BenchmarkRunner(chart, candles, container, metricsTracker);

let latestReport: BenchmarkReport | null = null;
(window as any).latestReport = null;
let setDataToFinishMs = 0;

// Generate 450k synthetic bars
console.log(`Generating ${BAR_COUNT} synthetic bars...`);
const t0 = performance.now();
const data = generateSyntheticCandles({ count: BAR_COUNT, seed: 1337 });
const genTime = performance.now() - t0;
console.log(`Generated ${BAR_COUNT} bars in ${genTime.toFixed(1)} ms`);

// Tightened measurement: setData -> draw -> gl.finish()
const tUploadStart = performance.now();
candles.setData(data);
chart.renderSync(true); // first draw and wait for GPU via gl.finish()
setDataToFinishMs = Number((performance.now() - tUploadStart).toFixed(2));

statUpload.textContent = `${setDataToFinishMs} ms`;
console.log(`setData + draw + gl.finish(): ${setDataToFinishMs} ms`);

statBars.textContent = BAR_COUNT.toLocaleString();

// Viewport change listener
chart.on('visibleRangeChange', (vp) => {
  statVisible.textContent = Math.round(vp.barsVisible).toLocaleString();
});

// Calculate honest memory breakdown
function getHonestMemory(): MemoryBreakdown {
  const mem = chart.getMemoryBreakdown();
  const toMb = (bytes: number) => Number((bytes / (1024 * 1024)).toFixed(2));

  let jsHeapMb: number | undefined;
  if ((performance as any).memory?.usedJSHeapSize) {
    jsHeapMb = toMb((performance as any).memory.usedJSHeapSize);
  }

  return {
    storeColumnsMb: toMb(mem.storeColumnsBytes),
    cpuPackedBufferMb: toMb(mem.cpuInstanceBufferBytes),
    gpuVboMb: toMb(mem.gpuVboBytes),
    totalMemoryMb: toMb(mem.totalEstimatedBytes),
    jsHeapMb,
  };
}

const initialMem = getHonestMemory();
statHeap.textContent = `${initialMem.totalMemoryMb} MB`;
console.log(`Memory breakdown:`, initialMem);

// Quick tick-to-painted baseline measurement
const quickTick = runner.measureLiveTickToPainted(50);
statTick.textContent = `${quickTick.p95Ms} ms`;
console.log(`Live tick to painted: p50=${quickTick.p50Ms} ms, p95=${quickTick.p95Ms} ms`);

// Periodic HUD update for FPS and Memory
let hudFrames = 0;
let hudLastTime = performance.now();
let hudP95 = 0;
const frameHistory: number[] = [];

function hudLoop(now: number) {
  hudFrames++;
  const dt = now - hudLastTime;
  frameHistory.push(dt);
  hudLastTime = now;

  if (frameHistory.length > 60) frameHistory.shift();

  if (hudFrames % 15 === 0 && frameHistory.length > 0) {
    const sorted = [...frameHistory].sort((a, b) => a - b);
    const p95Idx = Math.floor(sorted.length * 0.95);
    hudP95 = sorted[p95Idx];
    const avg = frameHistory.reduce((a, b) => a + b, 0) / frameHistory.length;
    const fps = avg > 0 ? Math.round(1000 / avg) : 0;

    statFps.textContent = `${fps}`;
    statP95.textContent = `${hudP95.toFixed(1)} ms`;

    const mem = getHonestMemory();
    statHeap.textContent = `${mem.totalMemoryMb} MB`;
  }

  requestAnimationFrame(hudLoop);
}
requestAnimationFrame(hudLoop);

// Theme Toggle
btnTheme.addEventListener('click', () => {
  currentTheme = currentTheme === darkTheme ? lightTheme : darkTheme;
  document.body.style.background = currentTheme.background;
  chart.destroy();
  const nextChart = createChart(container, { theme: currentTheme });
  const nextCandles = nextChart.addCandles({ capacity: 500_000 });
  nextCandles.setData(data);
});

// Benchmark execution helpers
function setButtonsDisabled(disabled: boolean) {
  const btns = [btnRunAll, btnPan, btnZoom, btnTick, btnAppend, btnResize];
  btns.forEach((b) => (b.disabled = disabled));
}

function showOverlay(title: string) {
  benchOverlay.classList.add('active');
  benchScenarioName.textContent = title;
  benchProgress.style.width = '0%';
  benchTimer.textContent = '0s / 10s';
}

async function executeSingleScenario(name: string, fn: () => Promise<any>) {
  setButtonsDisabled(true);
  showOverlay(`Running ${name}...`);

  const startTime = performance.now();
  const progressTimer = setInterval(() => {
    const elapsed = (performance.now() - startTime) / 1000;
    const pct = Math.min(100, (elapsed / 10) * 100);
    benchProgress.style.width = `${pct}%`;
    benchTimer.textContent = `${elapsed.toFixed(1)}s / 10s`;
  }, 100);

  try {
    const res = await fn();
    clearInterval(progressTimer);
    benchOutput.textContent = JSON.stringify(res, null, 2);
    btnExport.disabled = false;
    return res;
  } finally {
    clearInterval(progressTimer);
    setButtonsDisabled(false);
  }
}

// Button click handlers
btnPan.addEventListener('click', () => executeSingleScenario('Pan Scenario', () => runner.runPanScenario()));
btnZoom.addEventListener('click', () => executeSingleScenario('Zoom Sweep', () => runner.runZoomSweepScenario()));
btnTick.addEventListener('click', () => executeSingleScenario('Live Tick', () => runner.runLiveTickScenario()));
btnAppend.addEventListener('click', () => executeSingleScenario('Append Bar', () => runner.runAppendScenario()));
btnResize.addEventListener('click', () => executeSingleScenario('Resize Drag', () => runner.runResizeScenario()));

// Run All Scenarios
btnRunAll.addEventListener('click', async () => {
  setButtonsDisabled(true);
  benchOverlay.classList.add('active');

  const liveTickHist = runner.measureLiveTickToPainted(300);
  const appendHist = runner.measureAppendToPainted(100);
  const memoryBreakdown = getHonestMemory();

  const report: BenchmarkReport = {
    timestamp: new Date().toISOString(),
    gitCommit: 'm0-tightened-baseline',
    userAgent: navigator.userAgent,
    platform: navigator.platform,
    dpr: window.devicePixelRatio,
    windowSize: { width: window.innerWidth, height: window.innerHeight },
    gpuRenderer: getWebGLInfo(),
    barsCount: BAR_COUNT,
    setDataToPaintMs: setDataToFinishMs,
    setDataToFinishMs: setDataToFinishMs,
    liveTickToPainted: liveTickHist,
    appendLatency: appendHist,
    memory: memoryBreakdown,
    gateVerification: {
      setDataTarget150ms: setDataToFinishMs < 150 ? 'PASSED' : 'FAILED',
      liveTickTarget4ms: liveTickHist.p95Ms < 4.0 ? 'PASSED' : 'FAILED',
      appendTarget1ms: appendHist.p95Ms < 1.0 ? 'PASSED' : 'FAILED',
      memoryTarget60mb: memoryBreakdown.totalMemoryMb < 60 ? 'PASSED' : 'FAILED',
    },
    scenarios: {},
  };

  const scenarios = [
    { name: '1. Continuous Pan', fn: () => runner.runPanScenario() },
    { name: '2. Zoom Sweep', fn: () => runner.runZoomSweepScenario() },
    { name: '3. Live Tick 100Hz', fn: () => runner.runLiveTickScenario() },
    { name: '4. Append Bar 1Hz', fn: () => runner.runAppendScenario() },
    { name: '5. Resize Drag', fn: () => runner.runResizeScenario() },
  ];

  for (let i = 0; i < scenarios.length; i++) {
    const sc = scenarios[i];
    benchScenarioName.textContent = `[${i + 1}/5] ${sc.name}`;
    benchProgress.style.width = '0%';

    const startTime = performance.now();
    const timer = setInterval(() => {
      const elapsed = (performance.now() - startTime) / 1000;
      const pct = Math.min(100, (elapsed / 10) * 100);
      benchProgress.style.width = `${pct}%`;
      benchTimer.textContent = `${elapsed.toFixed(1)}s / 10s`;
    }, 100);

    const result = await sc.fn();
    clearInterval(timer);
    report.scenarios[result.scenarioName] = result;
    benchOutput.textContent = JSON.stringify(report, null, 2);
  }

  latestReport = report;
  (window as any).latestReport = report;
  btnExport.disabled = false;
  setButtonsDisabled(false);
  benchScenarioName.textContent = 'All Benchmarks Complete!';
});

// Export JSON
btnExport.addEventListener('click', () => {
  if (!latestReport) return;
  const jsonStr = JSON.stringify(latestReport, null, 2);
  const blob = new Blob([jsonStr], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `chartmelogne-benchmark-${Date.now()}.json`;
  a.click();
  URL.revokeObjectURL(url);
});
