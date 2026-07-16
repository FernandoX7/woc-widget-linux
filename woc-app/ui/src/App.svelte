<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { fade } from "svelte/transition";
  import Header from "./lib/components/chrome/Header.svelte";
  import PageSwitcher from "./lib/components/chrome/PageSwitcher.svelte";
  import Footer from "./lib/components/chrome/Footer.svelte";
  import { OverviewPage } from "./lib/components/overview";
  import { MarketCandlestickCard, MarketPage } from "./lib/components/market";
  import { CommunityPage } from "./lib/components/community";
  import PlayersChartCard from "./lib/components/overview/PlayersChartCard.svelte";
  import { SettingsPanel } from "./lib/components/settings";
  import type { SettingsFocusTarget } from "./lib/ipc/types";
  import { dismissWelcome, getDashboardSnapshot, listenForDashboardChanges, openExternal, quitApp, refreshCommunityFeed, refreshCommunityIfNeeded, refreshPage, refreshStatus, setCandleInterval, setChartRange, type DashboardPage, type DashboardSnapshot } from "./lib/ipc/dashboard";
  import type { ChartRange } from "./lib/overview/types";
  import { strings } from "./lib/strings";

  let { initialPage = "overview", initialSnapshot }: { initialPage?: DashboardPage; initialSnapshot?: DashboardSnapshot } = $props();
  const emptyFeed={value:null,phase:"loading" as const,lastSuccessMs:null};
  const empty: DashboardSnapshot = { realm: strings.realmFallback, playersOnline:null, realmState:"loading", statusFeedState:"loading", statusLastSuccessMs:null, statusRefreshing:false, price:null, change24h:null, marketQuote:null, candles:[], selectedCandleInterval:"5m", loadedCandleInterval:null, cryptoAlertThreshold:10, cryptoAlertWindow:"1h", quoteFeedState:"loading", quoteLastSuccessMs:null, quoteRefreshing:false, candleLastSuccessMs:null, candleFeedState:"loading", candleRefreshing:false, communityLastSuccessMs:null, communityFeedState:"loading", communityRefreshing:false, community:{projectStats:{...emptyFeed},releases:{...emptyFeed},leaderboard:{...emptyFeed},realms:{...emptyFeed},currentRealm:null}, snapshotTimeMs:Date.now(), chartRangeSeconds:21600, chartIntervalSeconds:300, chartIntervalLabel:"5m", chartPoints:[], chartYDomain:null, windowCoveragePercent:0, shortChange:null, todayHigh:null, localRecord:0, rangeAverage:null, rhythm:{sampleCount:0,coveragePercent:0,currentPercentile:null}, welcomeDismissed:true, populationAlertThreshold:125 };
  let snapshot = $state(untrack(() => initialSnapshot ?? empty));
  let page = $state<DashboardPage>(untrack(() => initialPage));
  let settingsOpen = $state(false);
  let reducedMotion = $state(false);
  let settingsFocus = $state<SettingsFocusTarget>(null);
  const openSettings = (target:SettingsFocusTarget=null) => { settingsFocus=target;settingsOpen=true; };
  const closeSettings = () => { settingsOpen=false;settingsFocus=null; };
  let manualRefresh = $state<DashboardPage | null>(null);
  const reload = async () => { snapshot = await getDashboardSnapshot(); };
  const rangeSeconds: Record<ChartRange, number> = { "1h":3600, "6h":21600, "24h":86400, "7d":604800 };
  const selectedRange = $derived((Object.entries(rangeSeconds).find(([, seconds]) => seconds === snapshot.chartRangeSeconds)?.[0] ?? "6h") as ChartRange);
  const observedChartPoints = $derived(snapshot.chartPoints.filter((point)=>point.value !== null));
  const chartAnalytics = $derived({ range:selectedRange, resolutionLabel:`${snapshot.chartIntervalLabel} averages`, points:snapshot.chartPoints, yDomain:snapshot.chartYDomain, coveragePercent:snapshot.windowCoveragePercent, accessibleDescription:strings.overview.chartSummary(selectedRange, observedChartPoints.length) });
  const changeRange = async (range: ChartRange) => { await setChartRange(rangeSeconds[range]); await reload(); };
  const dismiss = async () => { await dismissWelcome(); await reload(); };
  const refresh = async () => {
    const selected = page;
    manualRefresh = selected;
    try { await refreshPage(selected); await reload(); }
    finally { if (manualRefresh === selected) manualRefresh = null; }
  };
  const retryStatus = async () => { await refreshStatus(); await reload(); };
  const selectPage = (value:DashboardPage) => { page=value; if(value==="community") void refreshCommunityIfNeeded().then(reload); };
  const retryCommunity = async (feed: Parameters<typeof refreshCommunityFeed>[0]) => { await refreshCommunityFeed(feed); await reload(); };
  const candleSeconds = { "1m":60,"5m":300,"15m":900,"1h":3600,"4h":14400 } as const;
  const changeCandleInterval = async (interval: keyof typeof candleSeconds) => { await setCandleInterval(candleSeconds[interval]); await reload(); };
  const refreshing = $derived(manualRefresh === page || (page === "overview" ? snapshot.statusRefreshing || snapshot.quoteRefreshing : page === "market" ? snapshot.quoteRefreshing || snapshot.candleRefreshing : snapshot.communityRefreshing));
  const olderSuccess = (values: (number | null)[]) => values.filter((value): value is number => value !== null).reduce<number | null>((old, value) => old === null ? value : Math.min(old, value), null);
  const lastSuccess = $derived(page === "overview" ? olderSuccess([snapshot.statusLastSuccessMs, snapshot.quoteLastSuccessMs]) : page === "market" ? olderSuccess([snapshot.quoteLastSuccessMs,snapshot.candleLastSuccessMs]) : snapshot.communityLastSuccessMs);

  onMount(() => {
    reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
    let disposed = false;
    let unlisten: (()=>void)[] = [];
    void reload();
    if(page==="community") void refreshCommunityIfNeeded().then(reload);
    void listenForDashboardChanges(() => void reload()).then((value) => {
      if (disposed) value.forEach((stop) => stop());
      else unlisten = value;
    });
    return () => {
      disposed = true;
      unlisten.forEach((stop) => stop());
    };
  });
  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") { if (settingsOpen) closeSettings(); return; }
    if (!event.ctrlKey) return;
    if (event.key.toLowerCase() === "r") { event.preventDefault(); void refresh(); }
    else if (event.key === ",") { event.preventDefault(); openSettings(); }
    else if (event.key.toLowerCase() === "q") { event.preventDefault(); void quitApp(); }
  }
</script>

<svelte:window onkeydown={keydown} />
<main>
  {#if settingsOpen}
    <div class="view" transition:fade={{duration:reducedMotion?0:300}}><SettingsPanel close={closeSettings} focusTarget={settingsFocus}/></div>
  {:else}
   <div class="view" transition:fade={{duration:reducedMotion?0:300}}>
    <Header {snapshot}/><PageSwitcher {page} select={selectPage}/>
    {#if page === "overview"}
      {#snippet chart()}<PlayersChartCard analytics={chartAnalytics} disabled={snapshot.statusFeedState === "loading" && observedChartPoints.length === 0} onRangeChange={(range)=>void changeRange(range)} />{/snippet}
      <OverviewPage {chart} welcomeDismissed={snapshot.welcomeDismissed} realmState={snapshot.realmState} statusFeedState={snapshot.statusFeedState} shortChange={snapshot.shortChange} todayHigh={snapshot.todayHigh} localRecord={snapshot.localRecord} rangeAverage={snapshot.rangeAverage} rhythm={snapshot.rhythm} populationAlertThreshold={snapshot.populationAlertThreshold} dismissWelcome={()=>void dismiss()} refresh={()=>void retryStatus()} configurePopulationAlert={()=>openSettings("population")} openUrl={(url)=>void openExternal(url)} />
    {:else if page === "market"}
      {#snippet marketChart()}<MarketCandlestickCard candles={snapshot.candles} selectedInterval={snapshot.selectedCandleInterval} loadedInterval={snapshot.loadedCandleInterval} feedState={snapshot.candleFeedState} refreshing={snapshot.candleRefreshing} onIntervalChange={(interval)=>void changeCandleInterval(interval)}/>{/snippet}
      <MarketPage quote={snapshot.marketQuote} quoteState={snapshot.quoteFeedState} chartState={snapshot.candleFeedState} quoteLastSuccessMs={snapshot.quoteLastSuccessMs} candleLastSuccessMs={snapshot.candleLastSuccessMs} snapshotTimeMs={snapshot.snapshotTimeMs} alertThreshold={snapshot.cryptoAlertThreshold} alertWindow={snapshot.cryptoAlertWindow} selectedCandleInterval={snapshot.selectedCandleInterval} loadedCandleInterval={snapshot.loadedCandleInterval} chart={marketChart} retry={()=>void refresh()} openUrl={(url)=>void openExternal(url)} configureAlert={()=>openSettings("market-move")}/>
    {:else}
      <CommunityPage community={snapshot.community} snapshotTimeMs={snapshot.snapshotTimeMs} retry={(feed)=>void retryCommunity(feed)} openUrl={(url)=>void openExternal(url)} configureReleaseAlert={()=>openSettings("release-alerts")}/>
    {/if}
    <Footer {refreshing} lastSuccessMs={lastSuccess} snapshotTimeMs={snapshot.snapshotTimeMs} {refresh} settings={()=>openSettings()} quit={()=>void quitApp()}/>
   </div>
  {/if}
</main>

<style>
  :global(html),:global(body),:global(#app) { width:100%; height:100%; overflow:hidden; }
  main { width:var(--popover-width); height:var(--popover-height); overflow:hidden; display:flex; flex-direction:column; color:var(--color-text-primary); background:var(--gradient-popover-bg) padding-box,var(--gradient-popover-stroke) border-box; border:var(--line-075) solid transparent; border-radius:var(--radius-14); font-family:var(--font-rounded); }
  .view { position:absolute; inset:0; display:flex; flex-direction:column; overflow:hidden; }
</style>
