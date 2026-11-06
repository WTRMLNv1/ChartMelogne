export interface PriceTick {
  price: number;
  label: string;
}

export interface PriceTickOptions {
  priceDecimals?: number;
  tickSize?: number;
  targetSpacingPx?: number; // default 50px
}

/**
 * Computes the "nice number" step in {1, 2, 5} x 10^k closest to rawStep.
 */
export function getNicePriceStep(rawStep: number): number {
  if (rawStep <= 0 || !Number.isFinite(rawStep)) return 1;

  const exp = Math.floor(Math.log10(rawStep));
  const frac = rawStep / Math.pow(10, exp);

  let niceFrac: number;
  if (frac < 1.5) {
    niceFrac = 1;
  } else if (frac < 3.5) {
    niceFrac = 2;
  } else if (frac < 7.5) {
    niceFrac = 5;
  } else {
    niceFrac = 10;
  }

  return niceFrac * Math.pow(10, exp);
}

/**
 * Derives the number of decimal places needed to represent the step without floating point rounding artifacts.
 */
export function getPrecision(step: number): number {
  if (step >= 1) {
    return 2;
  }
  const s = step.toString();
  const dot = s.indexOf('.');
  if (dot === -1) return 0;
  return s.length - dot - 1;
}

/**
 * Generates monotonic, evenly-spaced price ticks within [yMin, yMax].
 */
export function generatePriceTicks(
  yMin: number,
  yMax: number,
  heightPx: number,
  options: PriceTickOptions = {}
): PriceTick[] {
  if (!Number.isFinite(yMin) || !Number.isFinite(yMax) || yMax <= yMin || heightPx <= 0) {
    return [];
  }

  const targetSpacing = options.targetSpacingPx ?? 50;
  const targetCount = Math.max(2, Math.floor(heightPx / targetSpacing));
  const rawStep = (yMax - yMin) / targetCount;
  const step = getNicePriceStep(rawStep);

  const decimals = options.priceDecimals !== undefined ? options.priceDecimals : Math.max(0, getPrecision(step));

  const start = Math.ceil(yMin / step) * step;
  const ticks: PriceTick[] = [];

  // Prevent infinite loops from floating point edge cases
  const maxIterations = 200;
  let current = start;
  let iter = 0;

  // Small epsilon to account for floating point imprecision
  const eps = step * 1e-6;

  while (current <= yMax + eps && iter < maxIterations) {
    // Avoid floating point accumulator jitter (e.g. 10.100000000000001)
    const roundedPrice = Number(current.toFixed(decimals));
    const normalizedPrice = Math.abs(roundedPrice) < eps ? 0 : roundedPrice;
    ticks.push({
      price: normalizedPrice,
      label: normalizedPrice.toFixed(decimals),
    });
    current += step;
    iter++;
  }

  return ticks;
}
