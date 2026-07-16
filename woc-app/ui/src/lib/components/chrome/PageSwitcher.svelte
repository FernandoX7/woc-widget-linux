<script lang="ts">
  import { strings } from "../../strings";
  import type { DashboardPage } from "../../ipc/dashboard";
  let { page, select }: { page: DashboardPage; select: (page: DashboardPage) => void } = $props();
  const pages: DashboardPage[] = ["overview", "market", "community"];
</script>
<nav aria-label={strings.dashboardPages}>
  {#each pages as item}<button class:active={page === item} onclick={() => select(item)}>{strings.pages[item]}</button>{/each}
</nav>
<style>
  nav { margin:var(--space-0) var(--space-16) var(--space-8); padding:var(--switcher-inset); display:grid; grid-template-columns:repeat(3,1fr); gap:var(--switcher-inset); background:var(--color-card); border:var(--line-1) solid var(--color-card-stroke); border-radius:var(--radius-8); }
  button { border:var(--space-0); border-radius:var(--radius-7); padding:var(--space-7); color:var(--color-text-tertiary); background:transparent; font-size:var(--type-button-label); font-weight:var(--weight-button-label); font-family:inherit; cursor:pointer; }
  button.active { color:var(--color-text-primary); background:var(--color-switcher-selection); }
</style>
