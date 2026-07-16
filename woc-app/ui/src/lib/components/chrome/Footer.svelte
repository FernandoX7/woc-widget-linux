<script lang="ts">
  import { strings } from "../../strings";
  let { refreshing, lastSuccessMs, snapshotTimeMs, refresh, settings, quit }: { refreshing:boolean; lastSuccessMs:number|null; snapshotTimeMs:number; refresh:()=>void; settings:()=>void; quit:()=>void } = $props();
  let now = $state(Date.now());
  $effect(() => {
    const offset = snapshotTimeMs - Date.now();
    now = snapshotTimeMs;
    const timer = window.setInterval(() => now = Date.now() + offset, 1000);
    return () => clearInterval(timer);
  });
  const seconds = $derived(lastSuccessMs === null ? null : Math.max(0, Math.floor((now-lastSuccessMs)/1000)));
  const relative = $derived(seconds === null ? strings.updatedNever : seconds === 0 ? strings.updatedNow : strings.updatedSeconds(seconds));
</script>
<footer>
  <button class="refresh glass-button glass-pill" onclick={refresh} disabled={refreshing} aria-label={strings.refresh} title={strings.refreshShortcut}>
    <span class:spin={refreshing}>↻</span><strong>{strings.refresh}</strong>
  </button>
  <span class="timestamp">{relative}</span>
  <div class="actions">
    <button class="icon glass-button" onclick={settings} aria-label={strings.settings} title={strings.settingsShortcut}>⚙</button>
    <button class="icon glass-button" onclick={quit} aria-label={strings.quit} title={strings.quitShortcut}>⏻</button>
  </div>
</footer>
<style>
  footer { flex:0 0 auto; min-height:var(--footer-min-height); display:flex; align-items:center; gap:var(--space-10); padding:var(--space-8) var(--space-16) var(--space-16); color:var(--color-text-tertiary); }
  button { cursor:pointer; }
  .refresh { --glass-pill-stroke:var(--color-cyan-30); display:flex; align-items:center; gap:var(--space-6); padding:var(--space-7) var(--space-12); color:var(--color-text-primary); font-size:var(--type-button-label); font-weight:var(--weight-button-label); }
  .refresh:disabled { cursor:default; opacity:var(--disabled-opacity); }
  .timestamp { font-size:var(--type-timestamp); font-weight:var(--weight-timestamp); }
  .actions { margin-left:auto; display:flex; gap:var(--space-8); }
  .icon { width:var(--icon-button-side); height:var(--icon-button-side); display:grid; place-items:center; border:var(--line-1) solid var(--color-card-stroke); border-radius:50%; color:var(--color-text-secondary); font-size:var(--type-icon-medium); background:var(--color-glass-fill); }
  .spin { display:inline-block; }
  @media (prefers-reduced-motion:no-preference) { .spin { animation:spin var(--motion-spinner) linear infinite; } }
  @keyframes spin { to { transform:rotate(360deg); } }
</style>
