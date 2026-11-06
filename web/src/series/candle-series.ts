import { CandleStore } from '../data/candle-store.js';
import type { Bar, CandleData, CandleSeriesOptions } from '../types.js';

export type SeriesDataChangeType = 'full' | 'append' | 'updateLast';

export class CandleSeries {
  private store: CandleStore;
  private onDataChanged?: (type: SeriesDataChangeType, index: number) => void;

  constructor(options: CandleSeriesOptions = {}, onDataChanged?: (type: SeriesDataChangeType, index: number) => void) {
    this.store = new CandleStore(options.capacity ?? 1024);
    this.onDataChanged = onDataChanged;
  }

  getStore(): CandleStore {
    return this.store;
  }

  get count(): number {
    return this.store.count;
  }

  setData(data: CandleData): void {
    this.store.setData(data);
    this.onDataChanged?.('full', 0);
  }

  append(bar: Bar): number {
    const idx = this.store.append(bar);
    this.onDataChanged?.('append', idx);
    return idx;
  }

  updateLast(bar: Bar): number {
    const idx = this.store.updateLast(bar);
    this.onDataChanged?.('updateLast', idx);
    return idx;
  }
}
