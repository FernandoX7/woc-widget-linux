import type { MarketCandle } from './types';

/** Swift CryptoFormat.chartPrice parity: four significant figures, fixed point. */
export function chartPrice(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0';
  const decimals = Math.min(12, Math.max(0, 3 - Math.floor(Math.log10(value))));
  return value.toFixed(decimals);
}

export function candleDomain(candles: MarketCandle[]): { min: number; max: number } | null {
  if (!candles.length) return null;
  const low = Math.min(...candles.map((candle) => candle.low));
  const high = Math.max(...candles.map((candle) => candle.high));
  const rawSpan = high - low;
  const span = rawSpan > 0 ? rawSpan : (high > 0 ? high : 1) * 0.02;
  const padding = span * 0.08;
  return { min: Math.max(0, low - padding), max: high + padding };
}
