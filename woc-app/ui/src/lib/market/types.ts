import type { FeedState } from '../ipc/types';

export type CandleInterval = '1m' | '5m' | '15m' | '1h' | '4h';

export interface MarketCandle {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface MarketCandlestickCardProps {
  candles: MarketCandle[];
  selectedInterval: CandleInterval;
  loadedInterval: CandleInterval | null;
  feedState: FeedState;
  refreshing?: boolean;
  onIntervalChange?: (interval: CandleInterval) => void;
}
