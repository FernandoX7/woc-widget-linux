<script lang="ts">
  import { strings } from "../../strings";

  let { shortChange, todayHigh, localRecord, rangeAverage }: {
    shortChange: number | null;
    todayHigh: number | null;
    localRecord: number | null;
    rangeAverage: number | null;
  } = $props();

  const signed = (value: number | null) => value === null
    ? strings.overview.stats.unavailable
    : `${value >= 0 ? "+" : ""}${value}`;
  const count = (value: number | null) => value === null ? strings.overview.stats.unavailable : String(value);
</script>

<dl class="stats" aria-label="Player statistics">
  <div><dt>{strings.overview.stats.shortChange}</dt><dd class:positive={shortChange !== null && shortChange > 0} class:negative={shortChange !== null && shortChange < 0}>{signed(shortChange)}</dd></div>
  <div><dt>{strings.overview.stats.todayHigh}</dt><dd>{count(todayHigh)}</dd></div>
  <div><dt>{strings.overview.stats.localRecord}</dt><dd>{count(localRecord)}</dd></div>
  <div><dt>{strings.overview.stats.rangeAverage}</dt><dd>{count(rangeAverage)}</dd></div>
</dl>

<style>
  .stats { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); margin:0; padding:var(--space-10) 0 0; border-top:var(--line-1) solid var(--color-card-stroke); }
  .stats div { min-width:0; text-align:center; border-left:var(--line-1) solid var(--color-card-stroke); }
  .stats div:first-child { border-left:0; }
  dt { overflow:hidden; color:var(--color-text-tertiary); font-size:var(--type-stat-label); font-weight:var(--weight-stat-label); text-overflow:ellipsis; text-transform:uppercase; white-space:nowrap; }
  dd { margin:var(--space-4) 0 0; color:var(--color-text-primary); font-family:var(--font-mono-digits); font-size:var(--type-stat-value); font-weight:var(--weight-stat-value); }
  .positive { color:var(--color-green); } .negative { color:var(--color-red); }
</style>
