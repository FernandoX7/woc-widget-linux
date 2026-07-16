<script lang="ts">
  import { strings } from "../../strings";
  import { signedPercent } from "../../format";
  import type { DashboardSnapshot } from "../../ipc/dashboard";

  let { snapshot }: { snapshot: DashboardSnapshot } = $props();
  let visible = $state(typeof document === "undefined" || document.visibilityState === "visible");
  const statusCached = $derived(snapshot.statusFeedState === "cached");
  const quoteCached = $derived(snapshot.quoteFeedState === "cached");
  const quoteUnavailable = $derived(snapshot.quoteFeedState === "unavailable");
  const quoteLoading = $derived(snapshot.quoteFeedState === "idle" || snapshot.quoteFeedState === "loading");
  const badge = $derived(statusCached ? strings.cached : snapshot.realmState === "healthy" ? strings.live : snapshot.realmState === "loading" ? strings.sync : snapshot.realmState === "unreachable" && snapshot.playersOnline !== null ? strings.cached : strings.offline);
  const status = $derived(statusCached ? strings.cached : snapshot.realmState === "healthy" ? strings.online : snapshot.realmState === "loading" ? strings.sync : snapshot.realmState === "unreachable" && snapshot.playersOnline !== null ? strings.cached : strings.offline);
</script>

<svelte:document onvisibilitychange={() => visible = document.visibilityState === "visible"} />

<header>
  <div class="signal">
    <div class="topline">
      <span class="realm">{(snapshot.realm || strings.realmFallback).toUpperCase()}</span>
      <span class="market glass-pill" class:cached={quoteCached} class:loading={quoteLoading} class:unavailable={quoteUnavailable}>
        {#if quoteCached}<span class="feed-icon" aria-hidden="true">◷</span>{:else if quoteUnavailable}<span class="wifi-icon" aria-hidden="true">◉</span>{:else if quoteLoading}<span class="feed-icon" aria-hidden="true">◷</span>{/if}
        {#if snapshot.price}<span>${snapshot.price}</span>{:else}<span>{strings.marketFallback}</span>{/if}
        {#if snapshot.change24h !== null}<small class:negative={snapshot.change24h < 0}>{signedPercent(snapshot.change24h)}</small>{/if}
      </span>
    </div>
    <div class="countline">
      {#key snapshot.playersOnline}<span class="count digit-roll" aria-label={strings.playerCount(snapshot.playersOnline ?? 0)}>{snapshot.playersOnline ?? "—"}</span>{/key}
      <span class="status">{status}</span>
    </div>
  </div>
  <span class="badge glass-pill" class:live={badge === strings.live} class:cached={badge === strings.cached} class:offline={badge === strings.offline}>
    <i class="status-dot" class:status-live={badge === strings.live} class:is-pulsing={badge === strings.live && visible}></i>{badge}
  </span>
</header>

<style>
  header { padding:var(--space-16) var(--space-16) var(--space-10); display:flex; align-items:center; gap:var(--space-12); flex:0 0 auto; }
  .signal { min-width:0; }
  .topline,.countline { display:flex; align-items:center; }
  .topline { gap:var(--space-6); }
  .realm { max-width:var(--realm-label-max-width); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:var(--type-title-heavy); font-weight:var(--weight-title-heavy); letter-spacing:var(--tracking-16); background:var(--gradient-brand-diagonal); color:transparent; background-clip:text; }
  .market { --glass-pill-stroke:var(--color-card-stroke); display:flex; align-items:center; gap:var(--space-4); padding:var(--space-2) var(--space-6); font-size:var(--type-section-label); font-weight:var(--weight-section-label); white-space:nowrap; }
  .market.cached,.market.loading { --glass-pill-stroke:var(--color-gold-25); }
  .market.unavailable { --glass-pill-stroke:var(--color-red-25); color:var(--color-red); }
  .feed-icon { color:var(--color-gold); }
  .wifi-icon { position:relative; color:var(--color-red); font-size:var(--space-0); width:var(--wifi-width); height:var(--wifi-height); border:var(--wifi-line-width) solid currentColor; border-left-color:transparent; border-right-color:transparent; border-bottom-color:transparent; border-radius:50%; transform:translateY(var(--space-2)); }
  .wifi-icon::after { content:""; position:absolute; left:var(--wifi-dot-offset-x); bottom:var(--space-1); width:var(--wifi-dot-side); height:var(--wifi-dot-side); border-radius:50%; background:currentColor; }
  .market small { color:var(--color-green); font-size:var(--type-crypto-change); font-weight:var(--weight-crypto-change); }
  .market small.negative { color:var(--color-red); }
  .countline { align-items:baseline; gap:var(--space-7); }
  .count { color:var(--color-text-primary); font-size:var(--type-count); line-height:1; font-weight:var(--weight-count); font-variant-numeric:tabular-nums; letter-spacing:var(--count-tracking); }
  .status { color:var(--color-text-secondary); font-size:var(--type-online); font-weight:var(--weight-online); transform:translateY(var(--online-baseline-offset)); }
  .badge { --glass-pill-stroke:var(--color-gold-25); margin-left:auto; display:flex; align-items:center; gap:var(--space-5); padding:var(--space-6) var(--space-9); color:var(--color-gold); font-size:var(--type-badge-label); font-weight:var(--weight-badge-label); letter-spacing:var(--tracking-08); white-space:nowrap; }
  .badge.live { --glass-pill-stroke:var(--color-cyan-40); color:var(--color-cyan); }
  .badge.offline { --glass-pill-stroke:var(--color-red-25); color:var(--color-red); }
  .badge .status-dot { width:var(--type-dot); height:var(--type-dot); --status-color:currentColor; box-shadow:0 0 var(--shadow-badge-dot-radius) currentColor; }
</style>
