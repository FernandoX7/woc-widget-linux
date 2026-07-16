export const fixtureScenarios = [
  'live',
  'welcome',
  'loading',
  'cached-offline',
  'quote-only',
  'chart-only',
  'empty-history',
  'gap-history',
  'rhythm-29',
  'rhythm-30',
  'community-all-healthy',
  'community-one-feed-failed',
  'community-all-loading',
  'community-cached',
] as const;

export type FixtureScenario = (typeof fixtureScenarios)[number];

export const dashboardPages = ['overview', 'market', 'community'] as const;
export type DashboardPage = (typeof dashboardPages)[number];

export type FeedState = 'idle' | 'loading' | 'live' | 'cached' | 'unavailable';

export interface FeedProvenance {
  state: FeedState;
  lastAttempt: string | null;
  lastSuccess: string | null;
  message: string | null;
}

export interface HistorySample {
  date: string;
  count: number;
}

export interface MarketWindow {
  changePercent: number;
  buys: number;
  sells: number;
  volumeUsd: number;
}

export interface MarketQuote {
  price: string;
  change24h: number;
  liquidityUsd: number;
  marketCapUsd: number | null;
  fullyDilutedValuationUsd: number;
  pairUrl: string;
  windows: Record<'5m' | '1h' | '6h' | '24h', MarketWindow>;
}

export interface Candle {
  date: string;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export interface CommunityFeedFixture<T> {
  value: T | null;
  phase: FeedState;
  lastSuccessMs: number | null;
}

export interface CommunityFixture {
  projectStats: CommunityFeedFixture<{
    accountsCreated?: number;
    playersOnline?: number;
    realm?: string;
  }>;
  releases: CommunityFeedFixture<Array<{
    id?: number;
    tag?: string;
    name?: string;
    body?: string;
    url?: string;
    prerelease: boolean;
    publishedAt?: string;
  }>>;
  leaderboard: CommunityFeedFixture<Array<{
    rank?: number;
    name?: string;
    cls?: string;
    level?: number;
    virtualLevel?: number;
    lifetimeXp?: number;
    prestigeRank?: number;
  }>>;
  realms: CommunityFeedFixture<Array<{ name?: string; url?: string; type?: string }>>;
  currentRealm: string | null;
}

export interface FixtureSnapshot {
  scenario: FixtureScenario;
  page: DashboardPage;
  referenceTime: string;
  welcomeDismissed: boolean;
  realm: {
    name: string;
    playersOnline: number | null;
    provenance: FeedProvenance;
  };
  market: {
    quote: MarketQuote | null;
    candles: Candle[];
    candleInterval: '1m' | '5m' | '15m' | '1h' | '4h';
    loadedCandleInterval: '1m' | '5m' | '15m' | '1h' | '4h' | null;
    quoteProvenance: FeedProvenance;
    chartProvenance: FeedProvenance;
  };
  history: HistorySample[];
  community: CommunityFixture;
  communityProvenance: FeedProvenance;
}
