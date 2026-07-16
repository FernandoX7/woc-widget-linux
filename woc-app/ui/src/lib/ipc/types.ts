export type DashboardPage = "overview" | "market" | "community";
export type FeedState = "idle" | "loading" | "live" | "cached" | "unavailable";
export type RealmState = "loading" | "healthy" | "offline" | "unreachable";
export interface MarketWindow { changePercent:number|null; buys:number|null; sells:number|null; volumeUsd:number|null }
export interface MarketQuote { price:string; change24h:number; liquidityUsd:number|null; marketCapUsd:number|null; fullyDilutedValuationUsd:number|null; pairUrl:string; windows:Partial<Record<"5m"|"1h"|"6h"|"24h",MarketWindow>> }
export interface MarketCandle { time:number; open:number; high:number; low:number; close:number; volume:number }
export type CandleInterval = "1m"|"5m"|"15m"|"1h"|"4h";
export type CommunityFeedName = "projectStats"|"releases"|"leaderboard"|"realms";
export interface CommunityFeed<T> { value:T|null; phase:FeedState; lastSuccessMs:number|null }
export interface CommunityRelease { id?:number; tag?:string; name?:string; body?:string; url?:string; prerelease:boolean; publishedAt?:string }
export interface CommunityLeader { rank?:number; name?:string; cls?:string; level?:number; virtualLevel?:number; lifetimeXp?:number; prestigeRank?:number }
export interface CommunityRealm { name?:string; url?:string; type?:string }
export interface CommunitySnapshot {
  projectStats:CommunityFeed<{ accountsCreated?:number; playersOnline?:number; realm?:string }>;
  releases:CommunityFeed<CommunityRelease[]>;
  leaderboard:CommunityFeed<CommunityLeader[]>;
  realms:CommunityFeed<CommunityRealm[]>;
  currentRealm:string|null;
}

export interface DashboardSnapshot {
  realm: string;
  playersOnline: number | null;
  realmState: RealmState;
  statusFeedState: FeedState;
  statusLastSuccessMs: number | null;
  statusRefreshing: boolean;
  price: string | null;
  change24h: number | null;
  marketQuote: MarketQuote | null;
  candles: MarketCandle[];
  selectedCandleInterval: CandleInterval;
  loadedCandleInterval: CandleInterval | null;
  cryptoAlertThreshold: number;
  cryptoAlertWindow: string;
  quoteFeedState: FeedState;
  quoteLastSuccessMs: number | null;
  quoteRefreshing: boolean;
  candleLastSuccessMs: number | null;
  candleFeedState: FeedState;
  candleRefreshing: boolean;
  communityLastSuccessMs: number | null;
  communityFeedState: FeedState;
  communityRefreshing: boolean;
  snapshotTimeMs: number;
  chartRangeSeconds: number;
  chartIntervalSeconds: number;
  chartIntervalLabel: string;
  chartPoints: Array<{ time: number; value: number | null; isolated: boolean }>;
  chartYDomain: { min: number; max: number } | null;
  windowCoveragePercent: number;
  shortChange: number | null;
  todayHigh: number | null;
  localRecord: number;
  rangeAverage: number | null;
  rhythm: { sampleCount: number; coveragePercent: number; currentPercentile: number | null };
  welcomeDismissed: boolean;
  populationAlertThreshold: number;
  community: CommunitySnapshot;
}

export type SettingsFocusTarget = "appearance" | "population" | "market-move" | "release-alerts" | null;

export interface SettingsSnapshot {
  alertsEnabled:boolean; peakAlertsEnabled:boolean; cryptoAlertsEnabled:boolean;
  tokenChangeGainAlertsEnabled:boolean; tokenChangeLossAlertsEnabled:boolean;
  pollSeconds:number; cryptoPollSeconds:number; cryptoAlertThreshold:number;
  cryptoAlertWindow:"oneHour"|"sixHours"|"twentyFourHours";
  populationThresholdAlertsEnabled:boolean; populationAlertThreshold:number;
  tokenPriceAboveAlertsEnabled:boolean; tokenPriceAboveTarget:number;
  tokenPriceBelowAlertsEnabled:boolean; tokenPriceBelowTarget:number;
  releaseAlertsEnabled:boolean; advancedAlertCooldown:number;
  advancedAlertQuietHoursEnabled:boolean; advancedAlertQuietStartMinute:number;
  advancedAlertQuietEndMinute:number; advancedAlertMutes:Record<string,number>;
  menuBarDisplayMode:"players"|"playersAndChange"|"token"|"full"|"iconOnly";
}

export const IPC_COMMANDS = {
  snapshot: "dashboard_snapshot",
  refreshPage: "refresh_page",
  refreshStatus: "refresh_status",
  quit: "quit_app",
  setChartRange: "set_chart_range",
  dismissWelcome: "dismiss_welcome",
  openExternal: "open_external",
  setCandleInterval: "set_candle_interval",
  refreshCommunityIfNeeded: "refresh_community_if_needed",
  refreshCommunityFeed: "refresh_community_feed",
  settingsSnapshot: "settings_snapshot",
  updateSetting: "update_setting",
  clearAlertMute: "clear_alert_mute",
  autostartEnabled: "autostart_enabled",
  setAutostartEnabled: "set_autostart_enabled",
  sendTestNotification: "send_test_notification",
  exportHistory: "export_history",
  clearHistory: "clear_history",
  historyPersistenceError: "history_persistence_error",
} as const;

export const IPC_EVENTS = {
  status: "woc://status-changed",
  market: "woc://market-changed",
  community: "woc://community-changed",
  settings: "woc://settings-changed",
  release: "woc://release-alert",
} as const;

export type IpcEvent = (typeof IPC_EVENTS)[keyof typeof IPC_EVENTS];
export type Unlisten = () => void;

export interface Transport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, handler: (payload: T) => void): Promise<Unlisten>;
}
