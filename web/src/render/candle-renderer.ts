import { createProgram } from './gl-utils.js';
import type { Theme } from '../types.js';

const VERTEX_SHADER_SRC = `#version 300 es
precision highp float;

// Unit geometry: x = side (-0.5..0.5), y = anchor (0.0..1.0), z = part (0=wick, 1=body)
in vec3 aUnitVertex;

// Instanced attributes: 16 bytes per candle (open, high, low, close relative to basePrice)
in vec4 aOHLC;

uniform float uFirstIndex; // bar index corresponding to gl_InstanceID == 0
uniform float uPxPerBar;
uniform float uFromIndex;
uniform vec2 uViewportPx;
uniform float uYScale;
uniform float uYOffset;
uniform float uBarsPerNode;
uniform vec4 uUpColor;
uniform vec4 uDownColor;
uniform float uDpr;

out vec4 vColor;

void main() {
  float isUp = float(aOHLC.w >= aOHLC.x);
  vColor = mix(uDownColor, uUpColor, isUp);

  // Compute bar index from gl_InstanceID and uFirstIndex (zero extra attribute!)
  float barIndex = float(gl_InstanceID) + uFirstIndex;

  // Candle horizontal center
  float centerX = (barIndex + 0.5 * uBarsPerNode - uFromIndex) * uPxPerBar;
  centerX = floor(centerX) + 0.5;

  // Body and wick dimensions in device pixels
  float bodyWidth = clamp(uPxPerBar * 0.7, 1.0, 30.0 * uDpr);
  float wickWidth = max(1.0, floor(max(1.0, 0.1 * bodyWidth) + 0.5));

  // Below 3 px per bar, drop bodies and only draw 1px wicks
  if (uPxPerBar < 3.0 * uDpr) {
    bodyWidth = 0.0;
    wickWidth = 1.0;
  }

  float o = aOHLC.x;
  float h = aOHLC.y;
  float l = aOHLC.z;
  float c = aOHLC.w;

  // Wick y range
  float yLow = (l - uYOffset) * uYScale;
  float yHigh = (h - uYOffset) * uYScale;
  float curYWick = mix(yLow, yHigh, aUnitVertex.y);

  // Body y range
  float bodyMin = min(o, c);
  float bodyMax = max(o, c);
  float yBottom = (bodyMin - uYOffset) * uYScale;
  float yTop = (bodyMax - uYOffset) * uYScale;

  // Minimum body height of 1 device pixel
  if (yTop - yBottom < 1.0) {
    float mid = (yBottom + yTop) * 0.5;
    yBottom = mid - 0.5;
    yTop = mid + 0.5;
  }
  float curYBody = mix(yBottom, yTop, aUnitVertex.y);

  // Select wick vs body
  float curY = mix(curYWick, curYBody, aUnitVertex.z);
  float curWidth = mix(wickWidth, bodyWidth, aUnitVertex.z);

  float curX = centerX + aUnitVertex.x * curWidth;

  // Map device pixels [0..width, 0..height] to WebGL clip space [-1, 1]
  float clipX = (curX / uViewportPx.x) * 2.0 - 1.0;
  float clipY = (curY / uViewportPx.y) * 2.0 - 1.0;

  gl_Position = vec4(clipX, clipY, 0.0, 1.0);
}
`;

const FRAGMENT_SHADER_SRC = `#version 300 es
precision mediump float;

in vec4 vColor;
out vec4 fragColor;

void main() {
  fragColor = vColor;
}
`;

// 12 vertices: 2 triangles for wick, 2 triangles for body
// Format: [xSide, yAnchor, part]
// prettier-ignore
const UNIT_VERTICES = new Float32Array([
  // Wick quad (part = 0.0)
  -0.5, 0.0, 0.0,
   0.5, 0.0, 0.0,
   0.5, 1.0, 0.0,
  -0.5, 0.0, 0.0,
   0.5, 1.0, 0.0,
  -0.5, 1.0, 0.0,

  // Body quad (part = 1.0)
  -0.5, 0.0, 1.0,
   0.5, 0.0, 1.0,
   0.5, 1.0, 1.0,
  -0.5, 0.0, 1.0,
   0.5, 1.0, 1.0,
  -0.5, 1.0, 1.0,
]);

export interface CandleRenderParams {
  fromIndex: number;
  barsVisible: number;
  yMin: number;
  yMax: number;
  basePrice: number;
  totalBars: number;
  viewportWidthPx: number;
  viewportHeightPx: number;
  dpr: number;
  theme: Theme;
  barsPerNode?: number;
}

export class CandleRenderer {
  private gl: WebGL2RenderingContext;
  private program: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private unitVbo: WebGLBuffer;
  private instanceVbo: WebGLBuffer;

  private aUnitVertexLoc: number;
  private aOHLCLoc: number;

  private uFirstIndexLoc: WebGLUniformLocation | null;
  private uPxPerBarLoc: WebGLUniformLocation | null;
  private uFromIndexLoc: WebGLUniformLocation | null;
  private uViewportPxLoc: WebGLUniformLocation | null;
  private uYScaleLoc: WebGLUniformLocation | null;
  private uYOffsetLoc: WebGLUniformLocation | null;
  private uBarsPerNodeLoc: WebGLUniformLocation | null;
  private uUpColorLoc: WebGLUniformLocation | null;
  private uDownColorLoc: WebGLUniformLocation | null;
  private uDprLoc: WebGLUniformLocation | null;

  private allocatedInstanceCapacity: number = 0;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    this.program = createProgram(gl, VERTEX_SHADER_SRC, FRAGMENT_SHADER_SRC);

    this.aUnitVertexLoc = gl.getAttribLocation(this.program, 'aUnitVertex');
    this.aOHLCLoc = gl.getAttribLocation(this.program, 'aOHLC');

    this.uFirstIndexLoc = gl.getUniformLocation(this.program, 'uFirstIndex');
    this.uPxPerBarLoc = gl.getUniformLocation(this.program, 'uPxPerBar');
    this.uFromIndexLoc = gl.getUniformLocation(this.program, 'uFromIndex');
    this.uViewportPxLoc = gl.getUniformLocation(this.program, 'uViewportPx');
    this.uYScaleLoc = gl.getUniformLocation(this.program, 'uYScale');
    this.uYOffsetLoc = gl.getUniformLocation(this.program, 'uYOffset');
    this.uBarsPerNodeLoc = gl.getUniformLocation(this.program, 'uBarsPerNode');
    this.uUpColorLoc = gl.getUniformLocation(this.program, 'uUpColor');
    this.uDownColorLoc = gl.getUniformLocation(this.program, 'uDownColor');
    this.uDprLoc = gl.getUniformLocation(this.program, 'uDpr');

    const vao = gl.createVertexArray();
    if (!vao) throw new Error('Failed to create VAO');
    this.vao = vao;

    gl.bindVertexArray(this.vao);

    // 1. Static Unit Geometry VBO
    const unitVbo = gl.createBuffer();
    if (!unitVbo) throw new Error('Failed to create unit VBO');
    this.unitVbo = unitVbo;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.unitVbo);
    gl.bufferData(gl.ARRAY_BUFFER, UNIT_VERTICES, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(this.aUnitVertexLoc);
    gl.vertexAttribPointer(this.aUnitVertexLoc, 3, gl.FLOAT, false, 0, 0);

    // 2. Dynamic Instance VBO (16 bytes per instance: vec4 aOHLC)
    const instanceVbo = gl.createBuffer();
    if (!instanceVbo) throw new Error('Failed to create instance VBO');
    this.instanceVbo = instanceVbo;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);

    gl.enableVertexAttribArray(this.aOHLCLoc);
    gl.vertexAttribDivisor(this.aOHLCLoc, 1);

    gl.bindVertexArray(null);
  }

  uploadData(ohlcBuffer: Float32Array, count: number): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);

    // 4 floats per bar * 4 bytes = 16 bytes per instance
    if (count > this.allocatedInstanceCapacity) {
      gl.bufferData(gl.ARRAY_BUFFER, ohlcBuffer.subarray(0, count * 4), gl.DYNAMIC_DRAW);
      this.allocatedInstanceCapacity = count;
    } else {
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, ohlcBuffer.subarray(0, count * 4));
    }
  }

  updateSubData(scratch4: Float32Array, index: number): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);
    // 16 bytes per instance
    gl.bufferSubData(gl.ARRAY_BUFFER, index * 16, scratch4);
  }

  render(params: CandleRenderParams): void {
    const {
      fromIndex,
      barsVisible,
      yMin,
      yMax,
      basePrice,
      totalBars,
      viewportWidthPx,
      viewportHeightPx,
      dpr,
      theme,
      barsPerNode = 1.0,
    } = params;

    if (totalBars <= 0 || viewportWidthPx <= 0 || viewportHeightPx <= 0) return;

    const gl = this.gl;

    // Visible sub-range calculation with padding
    const firstVisible = Math.max(0, Math.floor(fromIndex - 2));
    const lastVisible = Math.min(totalBars - 1, Math.ceil(fromIndex + barsVisible + 2));
    const instanceCount = lastVisible - firstVisible + 1;

    if (instanceCount <= 0) return;

    gl.useProgram(this.program);
    gl.bindVertexArray(this.vao);

    const pxPerBar = viewportWidthPx / barsVisible;
    const ySpan = yMax - yMin;
    const yScale = ySpan > 0 ? viewportHeightPx / ySpan : 1.0;
    const yOffset = yMin - basePrice;

    gl.uniform1f(this.uFirstIndexLoc, firstVisible);
    gl.uniform1f(this.uPxPerBarLoc, pxPerBar);
    gl.uniform1f(this.uFromIndexLoc, fromIndex);
    gl.uniform2f(this.uViewportPxLoc, viewportWidthPx, viewportHeightPx);
    gl.uniform1f(this.uYScaleLoc, yScale);
    gl.uniform1f(this.uYOffsetLoc, yOffset);
    gl.uniform1f(this.uBarsPerNodeLoc, barsPerNode);
    gl.uniform4fv(this.uUpColorLoc, theme.upColor);
    gl.uniform4fv(this.uDownColorLoc, theme.downColor);
    gl.uniform1f(this.uDprLoc, dpr);

    // Bind instance attributes with byte offset for sub-range rendering (16 bytes per candle)
    const byteOffset = firstVisible * 16;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);
    gl.vertexAttribPointer(this.aOHLCLoc, 4, gl.FLOAT, false, 16, byteOffset);

    // Draw instanced unit triangles: 12 vertices per instance
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 12, instanceCount);

    gl.bindVertexArray(null);
  }

  getGpuMemoryUsage(): { instanceVboBytes: number; unitVboBytes: number; totalGpuBytes: number } {
    // 16 bytes per instance
    const instanceVboBytes = this.allocatedInstanceCapacity * 16;
    const unitVboBytes = UNIT_VERTICES.byteLength;
    return {
      instanceVboBytes,
      unitVboBytes,
      totalGpuBytes: instanceVboBytes + unitVboBytes,
    };
  }

  finish(): void {
    this.gl.finish();
  }

  destroy(): void {
    const gl = this.gl;
    gl.deleteBuffer(this.unitVbo);
    gl.deleteBuffer(this.instanceVbo);
    gl.deleteVertexArray(this.vao);
    gl.deleteProgram(this.program);
  }
}
