import { createFixtureSnapshot } from './data';
import overviewAnalytics from './overview-analytics.json';
import { resolveFixtureSelection, type FixtureSelection } from './selection';
import type { DashboardSnapshot, Transport, Unlisten } from '../ipc/types';
import type { FixtureSnapshot } from './types';

export type FixtureTransport = Transport;

const EVENTS = new Set([
  'woc://status-changed',
  'woc://market-changed',
  'woc://community-changed',
  'woc://settings-changed',
  'woc://release-alert',
]);

const millis = (iso: string | null): number | null => (iso === null ? null : Date.parse(iso));

function dashboardSnapshot(snapshot: FixtureSnapshot, rangeSeconds = 21_600): DashboardSnapshot {
  const status = snapshot.realm.provenance;
  const quote = snapshot.market.quoteProvenance;
  const chart = snapshot.market.chartProvenance;
  const range = rangeSeconds === 3_600 ? '1h'
    : rangeSeconds === 86_400 ? '24h'
      : rangeSeconds === 604_800 ? '7d'
        : '6h';
  const analyticsScenario = (snapshot.scenario.startsWith('community-')
    ? 'live'
    : snapshot.scenario) as keyof typeof overviewAnalytics;
  const analytics = overviewAnalytics[analyticsScenario][range];
  return {
    realm: snapshot.realm.name,
    playersOnline: snapshot.realm.playersOnline,
    realmState:
      status.state === 'loading' ? 'loading' : status.state === 'cached' ? 'unreachable' : 'healthy',
    statusFeedState: status.state,
    statusLastSuccessMs: millis(status.lastSuccess),
    statusRefreshing: status.state === 'loading',
    price: snapshot.market.quote?.price ?? null,
    change24h: snapshot.market.quote?.change24h ?? null,
    marketQuote: snapshot.market.quote,
    candles: snapshot.market.candles.map(candle=>({ ...candle, time:Date.parse(candle.date)/1_000 })),
    selectedCandleInterval: snapshot.market.candleInterval,
    loadedCandleInterval: snapshot.market.loadedCandleInterval,
    cryptoAlertThreshold: 10,
    cryptoAlertWindow: "1h",
    quoteFeedState: quote.state,
    quoteLastSuccessMs: millis(quote.lastSuccess),
    quoteRefreshing: quote.state === 'loading',
    candleLastSuccessMs: millis(chart.lastSuccess),
    candleFeedState: chart.state,
    candleRefreshing: chart.state === 'loading',
    communityLastSuccessMs: millis(snapshot.communityProvenance.lastSuccess),
    communityFeedState: snapshot.communityProvenance.state,
    communityRefreshing: snapshot.communityProvenance.state === 'loading',
    community: snapshot.community,
    snapshotTimeMs: Date.parse(snapshot.referenceTime),
    ...analytics,
    welcomeDismissed: snapshot.welcomeDismissed,
    populationAlertThreshold: 125,
  };
}

export function createFixtureDashboardSnapshot(
  selection: FixtureSelection = resolveFixtureSelection(),
): DashboardSnapshot {
  return dashboardSnapshot(createFixtureSnapshot(selection.scenario, selection.page));
}

export function createFixtureTransport(selection: FixtureSelection = resolveFixtureSelection()): FixtureTransport {
  let snapshot = createFixtureSnapshot(selection.scenario, selection.page);
  let chartRangeSeconds = 21_600;
  const listeners = new Map<string, Set<(payload: unknown) => void>>();

  const publish = (event: string) => {
    listeners.get(event)?.forEach((listener) => listener(undefined));
  };

  return {
    async invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
      if (command === 'dashboard_snapshot') {
        const dashboard = dashboardSnapshot(snapshot, chartRangeSeconds);
        return structuredClone(dashboard) as T;
      }
      if (command === 'fixture_snapshot') return structuredClone(snapshot) as T;
      if (command === 'refresh_page') {
        const page = args.page;
        if (page !== 'overview' && page !== 'market' && page !== 'community') {
          throw new Error(`Unknown dashboard page: ${String(page)}`);
        }
        if (page === 'overview') {
          if (snapshot.scenario === 'loading') {
            snapshot = createFixtureSnapshot('live', 'overview');
          }
          publish('woc://status-changed');
          publish('woc://market-changed');
        }
        if (page === 'market') publish('woc://market-changed');
        if (page === 'community') publish('woc://community-changed');
        document.documentElement.dataset.fixtureRefreshedPage = page;
        return undefined as T;
      }
      if (command === 'refresh_community_if_needed') {
        publish('woc://community-changed');
        return undefined as T;
      }
      if (command === 'refresh_community_feed') {
        const feed = args.feed;
        if (feed !== 'projectStats' && feed !== 'releases' && feed !== 'leaderboard' && feed !== 'realms') {
          throw new Error(`Unknown community feed: ${String(feed)}`);
        }
        document.documentElement.dataset.fixtureCommunityFeed = feed;
        publish('woc://community-changed');
        return undefined as T;
      }
      if (command === 'refresh_status') {
        publish('woc://status-changed');
        return undefined as T;
      }
      if (command === 'quit_app') {
        document.documentElement.dataset.fixtureQuitRequested = 'true';
        return undefined as T;
      }
      if (command === 'set_chart_range') {
        const seconds = args.seconds;
        if (seconds !== 3_600 && seconds !== 21_600 && seconds !== 86_400 && seconds !== 604_800) throw new Error(`Unsupported chart range: ${String(seconds)}`);
        chartRangeSeconds = seconds;
        publish('woc://settings-changed');
        return undefined as T;
      }
      if (command === 'set_candle_interval') {
        const seconds = args.seconds;
        const interval = seconds === 60 ? '1m' : seconds === 300 ? '5m' : seconds === 900 ? '15m' : seconds === 3_600 ? '1h' : seconds === 14_400 ? '4h' : null;
        if (interval === null) throw new Error(`Unsupported candle interval: ${String(seconds)}`);
        snapshot.market.candleInterval = interval;
        snapshot.market.chartProvenance.state = 'cached';
        publish('woc://market-changed');
        return undefined as T;
      }
      if (command === 'dismiss_welcome') {
        snapshot.welcomeDismissed = true;
        publish('woc://settings-changed');
        return undefined as T;
      }
      if (command === 'open_external') {
        document.documentElement.dataset.fixtureOpenedUrl = String(args.url ?? '');
        return undefined as T;
      }
      throw new Error(`Unsupported fixture command: ${command}`);
    },
    async listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
      if (!EVENTS.has(event)) return () => undefined;
      const eventListeners = listeners.get(event) ?? new Set<(payload: unknown) => void>();
      const listener = handler as (payload: unknown) => void;
      eventListeners.add(listener);
      listeners.set(event, eventListeners);
      return () => eventListeners.delete(listener);
    },
  };
}
