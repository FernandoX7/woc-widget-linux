<script lang="ts">
  import { onDestroy, type Snippet } from "svelte";
  import type { FeedState, MarketQuote } from "../../ipc/types";
  import { signedPercent } from "../../format";
  import { strings } from "../../strings";

  type Timeframe = "5m" | "1h" | "6h" | "24h";
  let { quote, quoteState, chartState, quoteLastSuccessMs, candleLastSuccessMs, snapshotTimeMs,
    alertThreshold, alertWindow, selectedCandleInterval, loadedCandleInterval, chart, retry, openUrl, configureAlert }:
    { quote:MarketQuote|null; quoteState:FeedState; chartState:FeedState; quoteLastSuccessMs:number|null; candleLastSuccessMs:number|null; snapshotTimeMs:number; alertThreshold:number; alertWindow:string; selectedCandleInterval:string; loadedCandleInterval:string|null; chart:Snippet; retry:()=>void; openUrl:(url:string)=>void; configureAlert:()=>void } = $props();
  let timeframe = $state<Timeframe>("24h");
  let copied = $state(false);
  let copyGeneration = 0;
  let copyTimer: ReturnType<typeof setTimeout> | null = null;
  const window = $derived(quote?.windows[timeframe]);
  const change = $derived(window?.changePercent ?? (timeframe === "24h" ? quote?.change24h : null));
  const transactions = $derived((window?.buys ?? 0) + (window?.sells ?? 0));
  const buyShare = $derived(transactions > 0 ? (window?.buys ?? 0) / transactions : 0);
  const needsAttention = $derived(quoteState === "cached" || quoteState === "unavailable" || chartState === "cached" || chartState === "unavailable");
  const cachedAt = $derived(Math.min(...[quoteState === "cached" ? quoteLastSuccessMs : null, chartState === "cached" ? candleLastSuccessMs : null].filter((value):value is number=>value !== null)));

  const money = (value:number|null|undefined) => {
    if (value === null || value === undefined || !Number.isFinite(value) || value < 0) return "—";
    const [scaled, suffix] = value >= 1_000_000 ? [value / 1_000_000, "M"] : value >= 1_000 ? [value / 1_000, "K"] : [value, ""];
    return new Intl.NumberFormat("en-US", { style:"currency", currency:"USD", minimumFractionDigits:0, maximumFractionDigits:2 }).format(scaled as number) + suffix;
  };
  const price = (raw:string|null|undefined) => raw ? `$${raw}` : "—";
  const cachedAge = (time:number) => {
    const minutes = Math.max(0, Math.floor((snapshotTimeMs - time) / 60_000));
    if (minutes < 1) return strings.updatedNow;
    if (minutes < 60) return strings.market.cachedMinutes(minutes);
    return strings.market.cachedHours(Math.floor(minutes / 60));
  };
  const safePairUrl = (candidate:string|undefined) => {
    try { const url = new URL(candidate ?? ""); return url.protocol === "https:" && (url.hostname === "dexscreener.com" || url.hostname.endsWith(".dexscreener.com")) ? url.href : strings.market.dexUrl; }
    catch { return strings.market.dexUrl; }
  };
  const copyContract = async () => {
    await navigator.clipboard.writeText(strings.market.contract);
    const generation = ++copyGeneration;
    copied = true;
    if (copyTimer !== null) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => { if (generation === copyGeneration) copied=false; }, 2_000);
  };
  onDestroy(() => { if (copyTimer !== null) clearTimeout(copyTimer); });
</script>

<section class="market-page" aria-label={strings.market.pageLabel}>
  <article class="spot glass-card">
    <div class="spot-title"><h2>{strings.market.spot}</h2><button class="alert-stub glass-pill" onclick={configureAlert} aria-label={strings.market.alertAccessibility(alertThreshold,alertWindow)}>{alertThreshold}% · {alertWindow}</button></div>
    <div class="price-row"><strong>{price(quote?.price)}</strong>{#if change !== null && change !== undefined}<span class:gain={change >= 0} class:loss={change < 0}>{change >= 0 ? "↗" : "↘"} {signedPercent(change)}</span>{:else}<span class="missing">—</span>{/if}</div>
    <div class="divider"></div>
    <div class="timeframes" role="group" aria-label={strings.market.timeframe}>{#each ["5m","1h","6h","24h"] as option}<button aria-pressed={timeframe===option} onclick={()=>timeframe=option as Timeframe}>{option}</button>{/each}</div>
  </article>

  <div class="feed-notice">
    <div class="feed-row">{@render FeedPill(strings.market.quote,quoteState)}{@render FeedPill(strings.market.chart,chartState)}<span class="spacer"></span>{#if needsAttention}<button class="retry" onclick={retry}>{strings.market.retry}</button>{/if}</div>
    {#if chartState === "cached" && loadedCandleInterval !== null && loadedCandleInterval !== selectedCandleInterval}<p class="cached">◷ {strings.market.cachedCandles(loadedCandleInterval)}</p>{:else if quoteState === "cached" || chartState === "cached"}<p class="cached">◷ {Number.isFinite(cachedAt) ? cachedAge(cachedAt) : strings.market.showingCached}</p>{:else if quoteState === "unavailable" || chartState === "unavailable"}<p class="unavailable">▲ {strings.market.unavailableDetail}</p>{/if}
  </div>

  {@render chart()}

  <div class="metrics">
    {@render Metric(strings.market.volume(timeframe),money(window?.volumeUsd))}
    {@render Metric(strings.market.liquidity,money(quote?.liquidityUsd))}
    {@render Metric(strings.market.transactions(timeframe),window && (window.buys !== null || window.sells !== null) ? `${(window.buys ?? 0).toLocaleString()} / ${(window.sells ?? 0).toLocaleString()}` : "—")}
    {@render Metric(quote?.marketCapUsd == null ? strings.market.fdv : strings.market.marketCap,money(quote?.marketCapUsd ?? quote?.fullyDilutedValuationUsd))}
  </div>
  {#if transactions > 0}<div class="activity glass-card" aria-label={strings.market.activityAccessibility(timeframe,window?.buys ?? 0,window?.sells ?? 0)}><div><span class="gain">{strings.market.buyShare(buyShare)}</span><span class="loss">{strings.market.sellShare(1-buyShare)}</span></div><div class="activity-bar"><i class="buy" style={`width:${buyShare*100}%`}></i><i class="sell"></i></div></div>{/if}
  <div class="actions"><button class="view" onclick={()=>openUrl(safePairUrl(quote?.pairUrl))}>◉ {strings.market.view}</button><button class:copied onclick={()=>void copyContract()}>{copied ? "✓" : "▣"} {copied ? strings.market.contractCopied : strings.market.copyContract}</button></div>
  <button class="attribution" onclick={()=>openUrl(strings.market.geckoUrl)}>{strings.market.dataSource}</button>
</section>

{#snippet FeedPill(title:string,state:FeedState)}<span class="feed-pill state-{state}"><i></i>{title} <b>{strings.market.feedStates[state]}</b></span>{/snippet}
{#snippet Metric(label:string,value:string)}<div class="metric glass-card"><span>{label}</span><strong>{value}</strong></div>{/snippet}

<style>
  .market-page{flex:1;min-height:0;overflow:auto;padding:12px 16px;display:flex;flex-direction:column;gap:12px}.spot{padding:12px;display:grid;gap:10px}.spot-title,.price-row,.feed-row,.activity>div{display:flex;align-items:center}.spot-title h2{margin:0;font-size:var(--type-section-label);letter-spacing:.8px;color:var(--color-text-secondary)}.alert-stub{margin-left:auto;border-color:var(--color-card-stroke);color:var(--color-text-secondary);padding:4px 8px;font-size:var(--type-pill)}.price-row strong{font-size:var(--type-market-price);font-variant-numeric:tabular-nums}.price-row span{margin-left:auto;font-size:var(--type-emphasis);font-weight:700}.gain{color:var(--color-green)}.loss{color:var(--color-red)}.missing{color:var(--color-text-tertiary)}.divider{height:1px;background:var(--color-card-stroke)}.timeframes{display:grid;grid-template-columns:repeat(4,1fr);padding:3px;background:rgb(255 255 255/5%);border-radius:7px}.timeframes button{border:0;border-radius:5px;padding:3px;background:transparent;color:var(--color-text-secondary);font-size:11px}.timeframes button[aria-pressed=true]{background:var(--color-switcher-selection);color:var(--color-text-primary)}.feed-notice{padding:7px 10px;border:1px solid var(--color-card-stroke);border-radius:8px;background:rgb(255 255 255/3%)}.feed-row{gap:6px}.spacer{flex:1}.feed-pill{padding:4px 7px;border-radius:999px;background:rgb(255 255 255/4%);font-size:10px;color:var(--color-text-secondary)}.feed-pill i{display:inline-block;width:6px;height:6px;margin-right:4px;border-radius:50%;background:var(--color-cyan)}.feed-pill b{color:var(--color-cyan)}.state-live i{background:var(--color-green)}.state-live b{color:var(--color-green)}.state-cached i{background:var(--color-gold)}.state-cached b{color:var(--color-gold)}.state-unavailable i{background:var(--color-red)}.state-unavailable b{color:var(--color-red)}.retry,.attribution{border:0;background:none;color:var(--color-cyan);font-weight:600;font-size:12px}.feed-notice p{margin:6px 0 0;font-size:10.5px}.cached{color:var(--color-gold)}.unavailable{color:var(--color-red)}.metrics{display:grid;grid-template-columns:1fr 1fr;gap:8px}.metric{padding:10px;display:grid;gap:2px}.metric span{font-size:10px;color:var(--color-text-tertiary)}.metric strong{font-size:15px;font-variant-numeric:tabular-nums}.activity{padding:8px 10px;display:grid;gap:6px}.activity>div:first-child{justify-content:space-between;font-size:10px;font-weight:600}.activity-bar{display:flex;height:6px;gap:2px}.activity-bar i{border-radius:999px}.buy{background:rgb(33% 86% 49%/80%)}.sell{flex:1;background:rgb(96% 42% 42%/80%)}.actions{display:grid;grid-template-columns:1fr 1fr;gap:8px}.actions button{border:1px solid var(--color-card-stroke);border-radius:8px;padding:8px;background:var(--color-glass-fill);color:var(--color-text-secondary)}.actions .view{background:var(--gradient-brand-diagonal);color:white;border:0}.actions .copied{color:var(--color-green)}.attribution{align-self:center;color:var(--color-text-tertiary);font-size:10.5px;padding:0 0 4px}.market-page::-webkit-scrollbar{width:0}
</style>
