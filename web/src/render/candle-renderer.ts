import { createProgram } from './gl-utils.js';
import type { Theme } from '../types.js';

const VERTEX_SHADER_SRC = `#version 300 es
precision highp float;

// Unit geometry: x = side (-0.5..0.5), y = anchor (0.0..1.0), z = part (0=wick, 1=body)
in vec3 aUnitVertex;

// Instanced attributes
in float aBarIndex;
in vec4 aOHLC; // relative to basePrice

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

  // Candle horizontal center
  float centerX = (aBarIndex + 0.5 * uBarsPerNode - uFromIndex) * uPxPerBar;
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
  private aBarIndexLoc: number;
  private aOHLCLoc: number;

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
    this.aBarIndexLoc = gl.getAttribLocation(this.program, 'aBarIndex');
    this.aOHLCLoc = gl.getAttribLocation(this.program, 'aOHLC');

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

    // 2. Dynamic Instance VBO
    const instanceVbo = gl.createBuffer();
    if (!instanceVbo) throw new Error('Failed to create instance VBO');
    this.instanceVbo = instanceVbo;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);

    gl.enableVertexAttribArray(this.aBarIndexLoc);
    gl.vertexAttribDivisor(this.aBarIndexLoc, 1);

    gl.enableVertexAttribArray(this.aOHLCLoc);
    gl.vertexAttribDivisor(this.aOHLCLoc, 1);

    gl.bindVertexArray(null);
  }

  uploadData(gpuBuffer: Float32Array, count: number): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);

    // 5 floats per bar * 4 bytes = 20 bytes per instance
    const byteLength = count * 20;
    if (count > this.allocatedInstanceCapacity) {
      gl.bufferData(gl.ARRAY_BUFFER, gpuBuffer.subarray(0, count * 5), gl.DYNAMIC_DRAW);
      this.allocatedInstanceCapacity = count;
    } else {
      gl.bufferSubData(gl.ARRAY_BUFFER, 0, gpuBuffer.subarray(0, count * 5));
    }
  }

  updateSubData(gpuBuffer: Float32Array, startIndex: number, count: number = 1): void {
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);
    const startFloat = startIndex * 5;
    const endFloat = (startIndex + count) * 5;
    gl.bufferSubData(gl.ARRAY_BUFFER, startIndex * 20, gpuBuffer.subarray(startFloat, endFloat));
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

    gl.uniform1f(this.uPxPerBarLoc, pxPerBar);
    gl.uniform1f(this.uFromIndexLoc, fromIndex);
    gl.uniform2f(this.uViewportPxLoc, viewportWidthPx, viewportHeightPx);
    gl.uniform1f(this.uYScaleLoc, yScale);
    gl.uniform1f(this.uYOffsetLoc, yOffset);
    gl.uniform1f(this.uBarsPerNodeLoc, barsPerNode);
    gl.uniform4fv(this.uUpColorLoc, theme.upColor);
    gl.uniform4fv(this.uDownColorLoc, theme.downColor);
    gl.uniform1f(this.uDprLoc, dpr);

    // Bind instance attributes with byte offset for sub-range rendering (Section 7.1)
    const byteOffset = firstVisible * 20;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.instanceVbo);

    gl.vertexAttribPointer(this.aBarIndexLoc, 1, gl.FLOAT, false, 20, byteOffset);
    gl.vertexAttribPointer(this.aOHLCLoc, 4, gl.FLOAT, false, 20, byteOffset + 4);

    // Draw instanced unit triangles: 12 vertices per instance
    gl.drawArraysInstanced(gl.TRIANGLES, 0, 12, instanceCount);

    gl.bindVertexArray(null);
  }

  destroy(): void {
    const gl = this.gl;
    gl.deleteBuffer(this.unitVbo);
    gl.deleteBuffer(this.instanceVbo);
    gl.deleteVertexArray(this.vao);
    gl.deleteProgram(this.program);
  }
}
