<script lang="ts">
  import type { FeedState, RealmState } from "../../ipc/types"; import { strings } from "../../strings";
  let { realmState, feedState, retry }: { realmState: RealmState; feedState: FeedState; retry: () => void } = $props();
  const visible = $derived(realmState !== "healthy" || feedState === "cached");
  const tone = $derived(feedState === "cached" ? "cached" : realmState === "loading" ? "sync" : "offline");
  const title = $derived(tone === "cached" ? strings.overview.status.cachedTitle : tone === "sync" ? strings.overview.status.syncTitle : strings.overview.status.offlineTitle);
  const detail = $derived(tone === "cached" ? strings.overview.status.cachedDetail : tone === "sync" ? strings.overview.status.syncDetail : strings.overview.status.offlineDetail);
</script>
{#if visible}<aside class="banner {tone}" aria-live="polite"><div><strong>{title}</strong><span>{detail}</span></div><button onclick={retry}>{strings.overview.status.retry}</button></aside>{/if}
<style>
  .banner { display:flex; align-items:center; justify-content:space-between; gap:var(--space-10); padding:var(--space-8) var(--space-10); border:var(--line-1) solid currentColor; border-radius:var(--radius-8); font-size:var(--type-body-rounded); }
  .cached,.sync { color:var(--color-gold); background:var(--color-gold-25); } .offline { color:var(--color-red); background:var(--color-red-25); }
  .banner div { display:grid; gap:var(--space-2); } span { color:var(--color-text-secondary); } button { border:0; color:inherit; background:transparent; font:inherit; font-weight:var(--weight-button-label); cursor:pointer; }
</style>
