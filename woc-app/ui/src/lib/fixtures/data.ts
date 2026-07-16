import type {
  Candle,
  DashboardPage,
  FeedProvenance,
  FixtureScenario,
  FixtureSnapshot,
  HistorySample,
  MarketQuote,
} from './types';

export const FIXTURE_REFERENCE_TIME = '2026-07-11T18:00:00.000Z';
const REFERENCE_MS = Date.parse(FIXTURE_REFERENCE_TIME);
const CACHED_AGE_MS = 2 * 60 * 60 * 1000;
const cachedTime = new Date(REFERENCE_MS - CACHED_AGE_MS).toISOString();

const provenance = (
  state: FeedProvenance['state'],
  lastAttempt: string | null,
  lastSuccess: string | null,
  message: string | null = null,
): FeedProvenance => ({ state, lastAttempt, lastSuccess, message });

const history = (timeOffsetMs = 0): HistorySample[] =>
  Array.from({ length: 361 }, (_, index) => {
    const minutesAgo = 360 - index;
    const base = 82 + 14 * Math.sin(minutesAgo / 30) + 5 * Math.sin(minutesAgo / 7);
    const noise = ((minutesAgo * 17 + 11) % 7) - 3;
    return {
      date: new Date(REFERENCE_MS - minutesAgo * 60_000 + timeOffsetMs).toISOString(),
      count: Math.max(0, Math.round(base + noise)),
    };
  });

const quote: MarketQuote = {
  price: '0.0005594',
  change24h: 47.59,
  liquidityUsd: 71_688,
  marketCapUsd: 559_400,
  fullyDilutedValuationUsd: 559_400,
  pairUrl: 'https://dexscreener.com/solana/5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p',
  windows: {
    '5m': { changePercent: 1.2, buys: 8, sells: 5, volumeUsd: 912 },
    '1h': { changePercent: 6.8, buys: 72, sells: 48, volumeUsd: 9_420 },
    '6h': { changePercent: 18.4, buys: 321, sells: 264, volumeUsd: 48_200 },
    '24h': { changePercent: 47.59, buys: 526, sells: 508, volumeUsd: 81_075 },
  },
};

const candles = (timeOffsetMs = 0): Candle[] => {
  let price = 0.00052;
  return Array.from({ length: 60 }, (_, index) => {
    const offset = 59 - index;
    const open = price;
    const noise = ((offset * 13 + 5) % 7) - 3;
    const drift = 0.000008 * Math.sin(offset / 6) + 0.0000015 * noise;
    const close = Math.max(0.00001, open + drift);
    const high = Math.max(open, close) + 0.000002 * ((offset * 5) % 4);
    const low = Math.min(open, close) - 0.000002 * ((offset * 3) % 4);
    price = close;
    return {
      date: new Date(REFERENCE_MS - offset * 5 * 60_000 + timeOffsetMs).toISOString(),
      open,
      high,
      low,
      close,
      volume: 700 + ((offset * 137) % 1_900),
    };
  });
};

const community = {
  projectStats: {
    value: { accountsCreated: 12_847, playersOnline: 83, realm: 'Claudemoon' },
    phase: 'live' as const,
    lastSuccessMs: REFERENCE_MS,
  },
  currentRealm: 'Claudemoon',
  realms: {
    value: [
      { name: 'Claudemoon', url: 'https://claudemoon.example.com', type: 'seasonal' },
      { name: 'Sonnet', url: 'https://sonnet.example.com', type: 'standard' },
      { name: 'Opus', url: 'https://opus.example.com', type: 'standard' },
    ],
    phase: 'live' as const,
    lastSuccessMs: REFERENCE_MS,
  },
  releases: {
    value: [
      {
        id: 23,
        tag: 'v0.23.0',
        name: 'The Claudemoon Update',
        body: '# v0.23.0\n\n**Release:** v0.23.0\n**Date:** 2026-07-09\n**Previous release:** v0.22.0\n\nNew adventures, balance changes, and realm improvements.',
        publishedAt: '2026-07-09T15:30:00.000Z',
        url: 'https://github.com/levy-street/world-of-claudecraft/releases/tag/v0.23.0',
        prerelease: false,
      },
      { id: 22, tag: 'v0.22.0', name: 'Realm Rhythm', prerelease: false },
      { id: 21, tag: 'v0.21.0', name: 'Prestige Paths', prerelease: false },
    ],
    phase: 'live' as const,
    lastSuccessMs: REFERENCE_MS,
  },
  leaderboard: {
    value: [
      { rank: 1, name: 'Moonweaver', cls: 'mage', level: 90, virtualLevel: 94, lifetimeXp: 2_840_120, prestigeRank: 7 },
      { name: 'ClaudeKnight', level: 91, lifetimeXp: 2_611_480, prestigeRank: 0 },
      { rank: 3, name: 'Sonnet', cls: 'ranger' },
    ],
    phase: 'live' as const,
    lastSuccessMs: REFERENCE_MS,
  },
};

export function createFixtureSnapshot(
  scenario: FixtureScenario = 'live',
  page: DashboardPage = 'overview',
): FixtureSnapshot {
  const live = provenance('live', FIXTURE_REFERENCE_TIME, FIXTURE_REFERENCE_TIME);
  const loading = provenance('loading', FIXTURE_REFERENCE_TIME, null);
  const unavailableMarket = (message: string) =>
    provenance('unavailable', FIXTURE_REFERENCE_TIME, null, message);

  const snapshot: FixtureSnapshot = {
    scenario,
    page,
    referenceTime: FIXTURE_REFERENCE_TIME,
    welcomeDismissed: scenario !== 'welcome',
    realm: { name: 'Claudemoon', playersOnline: history().at(-1)?.count ?? 83, provenance: live },
    market: {
      quote,
      candles: candles(),
      candleInterval: '5m',
      loadedCandleInterval: '5m',
      quoteProvenance: live,
      chartProvenance: live,
    },
    history: history(),
    community: structuredClone(community),
    communityProvenance: live,
  };

  if (scenario === 'loading') {
    snapshot.realm = { name: 'Claudemoon', playersOnline: null, provenance: loading };
    snapshot.market = {
      quote: null,
      candles: [],
      candleInterval: '5m',
      loadedCandleInterval: null,
      quoteProvenance: loading,
      chartProvenance: loading,
    };
    snapshot.history = [];
    snapshot.communityProvenance = loading;
  } else if (scenario === 'cached-offline') {
    const cached = provenance('cached', FIXTURE_REFERENCE_TIME, cachedTime, 'This device is offline.');
    snapshot.realm.provenance = cached;
    snapshot.market.quoteProvenance = provenance(
      'cached', FIXTURE_REFERENCE_TIME, cachedTime, 'Market price is temporarily unavailable.',
    );
    snapshot.market.chartProvenance = provenance(
      'cached', FIXTURE_REFERENCE_TIME, cachedTime, 'Market chart is temporarily unavailable.',
    );
    snapshot.market.candles = candles(-CACHED_AGE_MS);
    snapshot.history = history(-CACHED_AGE_MS);
    snapshot.communityProvenance = cached;
  } else if (scenario === 'quote-only') {
    snapshot.market.quote = {
      ...quote,
      marketCapUsd: null,
      fullyDilutedValuationUsd: 999_000,
      pairUrl: 'https://dexscreener.com.evil.example/solana/hostile',
    };
    snapshot.market.candles = [];
    snapshot.market.chartProvenance = unavailableMarket('Market chart is temporarily unavailable.');
  } else if (scenario === 'chart-only') {
    snapshot.market.quote = null;
    snapshot.market.quoteProvenance = unavailableMarket('Market price is temporarily unavailable.');
  } else if (scenario === 'empty-history') {
    snapshot.history = [];
  } else if (scenario === 'gap-history') {
    const source = history();
    snapshot.history = [
      ...source.slice(0, 8),
      source[120],
      ...source.slice(240, 248),
    ];
  } else if (scenario === 'rhythm-29') {
    snapshot.history = history().slice(-29);
  } else if (scenario === 'rhythm-30') {
    snapshot.history = history().slice(-30);
  } else if (scenario === 'community-one-feed-failed') {
    snapshot.community.releases = { value: null, phase: 'unavailable', lastSuccessMs: null };
    snapshot.communityProvenance = provenance('cached', FIXTURE_REFERENCE_TIME, FIXTURE_REFERENCE_TIME);
  } else if (scenario === 'community-all-loading') {
    snapshot.community.projectStats = { value: null, phase: 'loading', lastSuccessMs: null };
    snapshot.community.releases = { value: null, phase: 'loading', lastSuccessMs: null };
    snapshot.community.leaderboard = { value: null, phase: 'loading', lastSuccessMs: null };
    snapshot.community.realms = { value: null, phase: 'loading', lastSuccessMs: null };
    snapshot.community.currentRealm = null;
    snapshot.communityProvenance = loading;
  } else if (scenario === 'community-cached') {
    const cachedMs = Date.parse(cachedTime);
    snapshot.community.projectStats = { ...snapshot.community.projectStats, phase: 'cached', lastSuccessMs: cachedMs };
    snapshot.community.releases = { ...snapshot.community.releases, phase: 'cached', lastSuccessMs: cachedMs };
    snapshot.community.leaderboard = { ...snapshot.community.leaderboard, phase: 'cached', lastSuccessMs: cachedMs };
    snapshot.community.realms = { ...snapshot.community.realms, phase: 'cached', lastSuccessMs: cachedMs };
    snapshot.communityProvenance = provenance('cached', FIXTURE_REFERENCE_TIME, cachedTime, 'This device is offline.');
  }

  return snapshot;
}
