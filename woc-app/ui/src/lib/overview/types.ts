/** Backend-produced chart analytics. The UI only renders these values. */
export type ChartRange = '1h' | '6h' | '24h' | '7d';

export interface PlayerChartPoint {
  /** Unix timestamp in seconds, aligned by wockit's analytics layer. */
  time: number;
  /** Null is a whitespace datum that prevents lines from bridging a gap. */
  value: number | null;
  /** True when wockit found a one-point segment. */
  isolated?: boolean;
}

export interface PlayerChartAnalytics {
  range: ChartRange;
  resolutionLabel: string;
  points: PlayerChartPoint[];
  yDomain: { min: number; max: number } | null;
  coveragePercent: number;
  accessibleDescription?: string;
}

