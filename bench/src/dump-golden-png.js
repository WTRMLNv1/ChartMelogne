import http from 'http';
import fs from 'fs';
import path from 'path';
import { spawn } from 'child_process';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const repoRoot = path.resolve(__dirname, '../..');
const webDistDir = path.resolve(repoRoot, 'web/dist');
const fixturesDir = path.resolve(repoRoot, 'bench/fixtures');
const outPngDir = path.resolve(fixturesDir, 'golden-png');

if (!fs.existsSync(outPngDir)) {
  fs.mkdirSync(outPngDir, { recursive: true });
}

const HTML_CONTENT = `<!DOCTYPE html>
<html>
<head><title>Dump Golden PNG</title></head>
<body>
<canvas id="c"></canvas>
<script type="module">
import { CandleStore } from '/web/dist/data/candle-store.js';
import { CandleRenderer } from '/web/dist/render/candle-renderer.js';
import { darkTheme } from '/web/dist/themes.js';

async function run() {
  const candlesRes = await fetch('/bench/fixtures/golden-candles.json');
  const candles = await candlesRes.json();
  const ticksRes = await fetch('/bench/fixtures/golden-viewport-ticks.json');
  const ticks = await ticksRes.json();

  const count = candles.time.length;
  const store = new CandleStore(count, 330);
  const typedCandles = {
    time: new Float64Array(candles.time),
    open: new Float64Array(candles.open),
    high: new Float64Array(candles.high),
    low: new Float64Array(candles.low),
    close: new Float64Array(candles.close),
    volume: new Uint32Array(candles.volume),
  };
  store.setData(typedCandles, true);

  const canvas = document.getElementById('c');
  const gl = canvas.getContext('webgl2', {
    alpha: true,
    antialias: false,
    powerPreference: 'high-performance',
    preserveDrawingBuffer: true,
  });

  const renderer = new CandleRenderer(gl);

  const levels = [
    { name: 'zoomed_in_intraday', fromIndex: 500, barsVisible: 50 },
    { name: 'medium_daily', fromIndex: 1000, barsVisible: 400 },
    { name: 'wide_multiweek', fromIndex: 2000, barsVisible: 2500 },
    { name: 'full_zoom_out', fromIndex: 0, barsVisible: 10000 },
  ];

  const dprs = [1, 1.5];

  for (const dpr of dprs) {
    const cssW = 1000;
    const cssH = 600;
    const widthPx = Math.round(cssW * dpr);
    const heightPx = Math.round(cssH * dpr);
    const priceAxisWidthPx = Math.round(65 * dpr);
    const timeAxisHeightPx = Math.round(28 * dpr);
    const plotWidthPx = Math.max(1, widthPx - priceAxisWidthPx);
    const plotHeightPx = Math.max(1, heightPx - timeAxisHeightPx);

    canvas.width = plotWidthPx;
    canvas.height = plotHeightPx;

    renderer.uploadData(store.ohlc, store.count);

    for (const lvl of levels) {
      const state = ticks[lvl.name].viewportState;

      gl.viewport(0, 0, plotWidthPx, plotHeightPx);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT);

      renderer.render({
        fromIndex: state.fromIndex,
        barsVisible: state.barsVisible,
        yMin: state.yMin,
        yMax: state.yMax,
        basePrice: store.basePrice,
        totalBars: store.count,
        viewportWidthPx: plotWidthPx,
        viewportHeightPx: plotHeightPx,
        dpr: dpr,
        theme: darkTheme,
      });

      gl.finish();

      const dataUrl = canvas.toDataURL('image/png');
      const filename = lvl.name + '_dpr' + dpr + '.png';

      await fetch('/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ filename, dataUrl })
      });
    }
  }

  await fetch('/done', { method: 'POST' });
}

run().catch(err => {
  console.error(err);
  fetch('/error', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ error: err.stack || String(err) })
  });
});
</script>
</body>
</html>
`;

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.json': 'application/json',
};

let savedCount = 0;

const server = http.createServer((req, res) => {
  const [reqPath] = req.url.split('?');

  if (req.method === 'POST' && reqPath === '/save') {
    let body = '';
    req.on('data', chunk => { body += chunk; });
    req.on('end', () => {
      const { filename, dataUrl } = JSON.parse(body);
      const base64Data = dataUrl.replace(/^data:image\/png;base64,/, '');
      const filePath = path.join(outPngDir, filename);
      fs.writeFileSync(filePath, Buffer.from(base64Data, 'base64'));
      savedCount++;
      console.log(`[${savedCount}/8] Saved golden PNG: ${filename}`);
      res.writeHead(200);
      res.end('OK');
    });
    return;
  }

  if (req.method === 'POST' && reqPath === '/done') {
    console.log('All 8 golden PNG fixtures generated successfully!');
    res.writeHead(200);
    res.end('OK');
    cleanup(0);
    return;
  }

  if (req.method === 'POST' && reqPath === '/error') {
    let body = '';
    req.on('data', chunk => { body += chunk; });
    req.on('end', () => {
      console.error('Browser error:', body);
      res.writeHead(500);
      res.end('Error');
      cleanup(1);
    });
    return;
  }

  if (reqPath === '/' || reqPath === '/index.html') {
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(HTML_CONTENT);
    return;
  }

  let filePath;
  if (reqPath.startsWith('/web/dist/')) {
    filePath = path.join(webDistDir, reqPath.slice('/web/dist/'.length));
  } else if (reqPath.startsWith('/bench/fixtures/')) {
    filePath = path.join(fixturesDir, reqPath.slice('/bench/fixtures/'.length));
  }

  if (filePath && fs.existsSync(filePath) && fs.statSync(filePath).isFile()) {
    const ext = path.extname(filePath);
    res.writeHead(200, { 'Content-Type': MIME_TYPES[ext] || 'application/octet-stream' });
    fs.createReadStream(filePath).pipe(res);
  } else {
    res.writeHead(404);
    res.end('Not found: ' + reqPath);
  }
});

let browserProc = null;

function cleanup(code) {
  if (browserProc) {
    try { browserProc.kill(); } catch (e) {}
  }
  server.close(() => {
    process.exit(code);
  });
}

server.listen(0, '127.0.0.1', () => {
  const port = server.address().port;
  console.log(`Server listening at http://127.0.0.1:${port}`);

  const edgePath = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
  browserProc = spawn(edgePath, [
    '--headless=new',
    '--disable-gpu-vsync',
    '--use-gl=angle',
    '--enable-webgl',
    `http://127.0.0.1:${port}/`,
  ]);

  browserProc.on('error', err => {
    console.error('Failed to start Edge:', err);
    cleanup(1);
  });

  // 30s watchdog
  setTimeout(() => {
    console.error('Timeout waiting for golden PNG dump');
    cleanup(1);
  }, 30000);
});
