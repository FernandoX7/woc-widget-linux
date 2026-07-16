<script lang="ts">
  import type { Snippet } from "svelte";
  import type { FeedState, RealmState } from "../../ipc/types";
  import OverviewStats from "./OverviewStats.svelte";
  import QuickLinks from "./QuickLinks.svelte";
  import RealmRhythm from "./RealmRhythm.svelte";
  import StatusBanner from "./StatusBanner.svelte";
  import WelcomeCard from "./WelcomeCard.svelte";

  let {
    chart,
    welcomeDismissed,
    realmState,
    statusFeedState,
    shortChange,
    todayHigh,
    localRecord,
    rangeAverage,
    rhythm,
    populationAlertThreshold,
    dismissWelcome,
    refresh,
    configurePopulationAlert,
    openUrl,
  }: {
    chart: Snippet;
    welcomeDismissed: boolean;
    realmState: RealmState;
    statusFeedState: FeedState;
    shortChange: number | null;
    todayHigh: number | null;
    localRecord: number | null;
    rangeAverage: number | null;
    rhythm: { sampleCount: number; coveragePercent: number; currentPercentile: number | null };
    populationAlertThreshold: number;
    dismissWelcome: () => void;
    refresh: () => void;
    configurePopulationAlert: () => void;
    openUrl: (url: string) => void;
  } = $props();
</script>

<section class="overview" aria-label="Overview">
  {#if !welcomeDismissed}<WelcomeCard dismiss={dismissWelcome} />{/if}
  <StatusBanner {realmState} feedState={statusFeedState} retry={refresh} />
  <section class="chart-card">
    {@render chart()}
    <OverviewStats {shortChange} {todayHigh} {localRecord} {rangeAverage} />
  </section>
  <RealmRhythm {...rhythm} {populationAlertThreshold} configureAlert={configurePopulationAlert} />
  <QuickLinks {openUrl} />
</section>

<style>
  .overview { min-height:0; display:flex; flex-direction:column; gap:var(--space-10); padding:var(--space-10) var(--space-12) var(--space-12); overflow-y:auto; }
  .chart-card { padding:var(--space-12); border:var(--line-1) solid var(--color-card-stroke); border-radius:var(--radius-8); background:var(--color-card); }
</style>
