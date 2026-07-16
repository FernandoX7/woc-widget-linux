<script lang="ts">
  import { strings } from "../../strings";
  let { sampleCount, coveragePercent, currentPercentile, populationAlertThreshold, configureAlert }: {
    sampleCount: number;
    coveragePercent: number;
    currentPercentile: number | null;
    populationAlertThreshold: number;
    configureAlert: () => void;
  } = $props();
</script>

<section class="card" aria-labelledby="rhythm-title">
  <div class="heading"><h2 id="rhythm-title">{strings.overview.rhythm.title}</h2><button onclick={configureAlert} title={strings.overview.rhythm.alertSetup}>{strings.overview.rhythm.alertAt(populationAlertThreshold)}</button></div>
  <p class="message">{currentPercentile === null ? strings.overview.rhythm.gathering(sampleCount) : strings.overview.rhythm.percentile(currentPercentile)}</p>
  <div class="coverage"><progress max="100" value={coveragePercent} aria-label={strings.overview.rhythm.observed(coveragePercent)}></progress><span>{strings.overview.rhythm.observed(coveragePercent)}</span></div>
  <p class="footnote">{strings.overview.rhythm.footnote}</p>
</section>

<style>
  .card { padding:var(--space-12); border:var(--line-1) solid var(--color-card-stroke); border-radius:var(--radius-8); background:var(--color-card); }
  .heading,.coverage { display:flex; align-items:center; justify-content:space-between; gap:var(--space-10); }
  h2 { margin:0; font-size:var(--type-section-label); font-weight:var(--weight-section-label); letter-spacing:var(--tracking-16); }
  button { padding:var(--space-5) var(--space-8); border:var(--line-1) solid var(--color-cyan-40); border-radius:var(--pill-radius); color:var(--color-cyan); background:var(--color-cyan-30); font:inherit; font-size:var(--type-pill); cursor:pointer; }
  .message { margin:var(--space-10) 0; font-size:var(--type-emphasis); font-weight:var(--weight-emphasis); }
  progress { width:100%; height:var(--space-6); overflow:hidden; border:0; border-radius:var(--pill-radius); background:var(--color-card-stroke); appearance:none; -webkit-appearance:none; }
  progress::-webkit-progress-bar { border-radius:var(--pill-radius); background:var(--color-card-stroke); }
  progress::-webkit-progress-value { border-radius:var(--pill-radius); background:linear-gradient(90deg,var(--color-cyan),var(--color-violet)); }
  progress::-moz-progress-bar { border-radius:var(--pill-radius); background:linear-gradient(90deg,var(--color-cyan),var(--color-violet)); }
  .coverage span,.footnote { color:var(--color-text-tertiary); font-size:var(--type-placeholder-hint); }
  .coverage span { flex:none; }
  .footnote { margin:var(--space-7) 0 0; }
</style>
