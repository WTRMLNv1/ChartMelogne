export interface InteractionCallbacks {
  onPan: (deltaScreenPx: number) => void;
  onZoom: (cursorScreenPx: number, zoomFactor: number) => void;
  onPointerMove?: (screenX: number, screenY: number) => void;
  onPointerLeave?: () => void;
  onClick?: (screenX: number, screenY: number) => void;
}

export class InteractionHandler {
  private element: HTMLElement;
  private callbacks: InteractionCallbacks;
  private getDpr: () => number;

  private isDragging = false;
  private lastPointerX = 0;
  private lastPointerY = 0;
  private activeTouchPointers = new Map<number, { x: number; y: number }>();
  private initialPinchDist = 0;

  constructor(element: HTMLElement, callbacks: InteractionCallbacks, getDpr: () => number) {
    this.element = element;
    this.callbacks = callbacks;
    this.getDpr = getDpr;

    this.attach();
  }

  private attach(): void {
    const el = this.element;
    el.style.touchAction = 'none'; // prevent browser default pinch/scroll

    el.addEventListener('pointerdown', this.onPointerDown, { passive: false });
    el.addEventListener('pointermove', this.onPointerMove, { passive: false });
    el.addEventListener('pointerup', this.onPointerUp, { passive: false });
    el.addEventListener('pointercancel', this.onPointerUp, { passive: false });
    el.addEventListener('pointerleave', this.onPointerLeave, { passive: false });
    el.addEventListener('wheel', this.onWheel, { passive: false });
  }

  detach(): void {
    const el = this.element;
    el.removeEventListener('pointerdown', this.onPointerDown);
    el.removeEventListener('pointermove', this.onPointerMove);
    el.removeEventListener('pointerup', this.onPointerUp);
    el.removeEventListener('pointercancel', this.onPointerUp);
    el.removeEventListener('pointerleave', this.onPointerLeave);
    el.removeEventListener('wheel', this.onWheel);
  }

  private onPointerDown = (e: PointerEvent): void => {
    if (e.pointerType === 'touch') {
      this.activeTouchPointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      if (this.activeTouchPointers.size === 2) {
        const [p1, p2] = Array.from(this.activeTouchPointers.values());
        this.initialPinchDist = Math.hypot(p2.x - p1.x, p2.y - p1.y);
        return;
      }
    }

    if (e.button === 0) {
      // Primary button
      this.isDragging = true;
      this.lastPointerX = e.clientX;
      this.lastPointerY = e.clientY;
      try {
        this.element.setPointerCapture(e.pointerId);
      } catch {}
    }
  };

  private onPointerMove = (e: PointerEvent): void => {
    const dpr = this.getDpr();
    const rect = this.element.getBoundingClientRect();
    const screenX = (e.clientX - rect.left) * dpr;
    const screenY = (rect.bottom - e.clientY) * dpr; // bottom-up

    if (e.pointerType === 'touch' && this.activeTouchPointers.has(e.pointerId)) {
      this.activeTouchPointers.set(e.pointerId, { x: e.clientX, y: e.clientY });

      if (this.activeTouchPointers.size === 2) {
        const [p1, p2] = Array.from(this.activeTouchPointers.values());
        const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y);
        if (this.initialPinchDist > 0 && dist > 0) {
          const midX = ((p1.x + p2.x) * 0.5 - rect.left) * dpr;
          const factor = this.initialPinchDist / dist;
          this.callbacks.onZoom(midX, factor);
          this.initialPinchDist = dist;
        }
        return;
      }
    }

    if (this.isDragging) {
      const deltaCssX = e.clientX - this.lastPointerX;
      this.lastPointerX = e.clientX;
      this.lastPointerY = e.clientY;
      this.callbacks.onPan(deltaCssX * dpr);
    }

    this.callbacks.onPointerMove?.(screenX, screenY);
  };

  private onPointerUp = (e: PointerEvent): void => {
    if (e.pointerType === 'touch') {
      this.activeTouchPointers.delete(e.pointerId);
      if (this.activeTouchPointers.size < 2) {
        this.initialPinchDist = 0;
      }
    }

    if (this.isDragging) {
      this.isDragging = false;
      try {
        this.element.releasePointerCapture(e.pointerId);
      } catch {}
    }
  };

  private onPointerLeave = (): void => {
    this.callbacks.onPointerLeave?.();
  };

  private onWheel = (e: WheelEvent): void => {
    e.preventDefault();
    const dpr = this.getDpr();
    const rect = this.element.getBoundingClientRect();
    const cursorScreenPx = (e.clientX - rect.left) * dpr;

    // Shift key or horizontal wheel gesture: horizontal pan
    if (e.shiftKey || Math.abs(e.deltaX) > Math.abs(e.deltaY)) {
      const delta = (e.shiftKey ? e.deltaY : e.deltaX) * dpr;
      this.callbacks.onPan(-delta);
      return;
    }

    // Anchored zoom
    // Normalize delta across browsers
    const normalizedDelta = Math.sign(e.deltaY) * Math.min(Math.abs(e.deltaY), 100);
    const zoomFactor = Math.exp(normalizedDelta * 0.0018);
    this.callbacks.onZoom(cursorScreenPx, zoomFactor);
  };
}
