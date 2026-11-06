import http from 'http';
import fs from 'fs';
import path from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const distDir = path.resolve(__dirname, '../dist');

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
};

// 1. Static file server
const server = http.createServer((req, res) => {
  let reqPath = req.url.split('?')[0];
  if (reqPath === '/') reqPath = '/index.html';
  const filePath = path.join(distDir, reqPath);

  if (fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
    const ext = path.extname(filePath);
    res.writeHead(200, { 'Content-Type': MIME_TYPES[ext] || 'application/octet-stream' });
    fs.createReadStream(filePath).pipe(res);
  } else {
    res.writeHead(404);
    res.end('Not found');
  }
});

server.listen(0, '127.0.0.1', async () => {
  const port = server.address().port;
  console.log(`HTTP server listening on http://127.0.0.1:${port}`);

  const edgePath = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
  const debugPort = 9222 + Math.floor(Math.random() * 500);

  console.log(`Launching Edge in headless mode...`);
  const browserProc = spawn(edgePath, [
    '--headless=new',
    '--disable-gpu-vsync',
    '--use-gl=angle',
    '--enable-webgl',
    '--enable-features=Vulkan',
    `--remote-debugging-port=${debugPort}`,
    `http://127.0.0.1:${port}/`,
  ]);

  // Wait for browser debug port to be available
  await new Promise((r) => setTimeout(r, 2000));

  try {
    const targetsRes = await fetch(`http://127.0.0.1:${debugPort}/json`);
    const targets = await targetsRes.json();
    const pageTarget = targets.find((t) => t.type === 'page');

    if (!pageTarget) {
      throw new Error('No page target found');
    }

    console.log(`Connected to page target: ${pageTarget.title}`);
    const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);

    let id = 1;
    const send = (method, params = {}) => {
      const msgId = id++;
      ws.send(JSON.stringify({ id: msgId, method, params }));
      return msgId;
    };

    ws.onopen = () => {
      send('Runtime.enable');
      send('Console.enable');
      console.log('Runtime & Console enabled, listening for benchmark progress...');
    };

    ws.onmessage = async (event) => {
      const msg = JSON.parse(event.data);
      if (msg.method === 'Runtime.consoleAPICalled') {
        const text = msg.params.args.map((a) => a.value || JSON.stringify(a.value)).join(' ');
        console.log(`[Browser Console] ${text}`);
      }

      // Check if benchmark has completed every second
      if (msg.id === 999 && msg.result?.result?.value) {
        const report = JSON.parse(msg.result.result.value);
        console.log('\n=== REAL BROWSER BENCHMARK RESULTS (M0 Baseline) ===');
        console.log(`GPU: ${report.gpuRenderer}`);
        console.log(`Bars: ${report.barsCount.toLocaleString()}`);
        console.log(`setData + draw + gl.finish(): ${report.setDataToFinishMs} ms`);
        console.log(`Live Tick to Painted (gl.finish): p50=${report.liveTickToPainted.p50Ms} ms, p95=${report.liveTickToPainted.p95Ms} ms, max=${report.liveTickToPainted.maxMs} ms`);
        console.log(`Append to Painted (gl.finish): p50=${report.appendLatency.p50Ms} ms, p95=${report.appendLatency.p95Ms} ms, max=${report.appendLatency.maxMs} ms`);
        console.log(`Memory Breakdown:`);
        console.log(`  - Store Columns (F64): ${report.memory.storeColumnsMb} MB`);
        console.log(`  - CPU Packed F32 Buffer: ${report.memory.cpuPackedBufferMb} MB`);
        console.log(`  - GPU Instance VBO: ${report.memory.gpuVboMb} MB`);
        console.log(`  - Total Data Footprint: ${report.memory.totalMemoryMb} MB`);
        if (report.memory.jsHeapMb) {
          console.log(`  - V8 JS Heap: ${report.memory.jsHeapMb} MB`);
        }
        console.log(`\nGates Verification:`, JSON.stringify(report.gateVerification, null, 2));

        const resultsDir = path.resolve(__dirname, '../results');
        if (!fs.existsSync(resultsDir)) fs.mkdirSync(resultsDir, { recursive: true });
        const outPath = path.join(resultsDir, 'baseline-m0.json');
        fs.writeFileSync(outPath, JSON.stringify(report, null, 2), 'utf-8');
        console.log(`\nUpdated baseline report written to: ${outPath}`);

        ws.close();
        browserProc.kill();
        server.close();
        process.exit(0);
      }
    };

    // After 2s, click the "Run All" button or execute the benchmark programmatically
    setTimeout(() => {
      console.log('Triggering "Run All" in browser...');
      send('Runtime.evaluate', {
        expression: `
          (async () => {
            const btn = document.getElementById('btn-run-all');
            if (btn) btn.click();
          })()
        `,
      });
    }, 2500);

    // Periodically poll for completion
    const pollInterval = setInterval(() => {
      send('Runtime.evaluate', {
        id: 999,
        expression: `latestReport ? JSON.stringify(latestReport) : null`,
        returnByValue: true,
      });
    }, 1500);
  } catch (err) {
    console.error('Error during benchmark execution:', err);
    browserProc.kill();
    server.close();
    process.exit(1);
  }
});
