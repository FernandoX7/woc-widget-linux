<script lang="ts">import { strings } from "../../strings"; let { dismiss }: { dismiss: () => void } = $props();</script>
<section class="welcome"><div><h2>{strings.overview.welcomeTitle}</h2><p>{strings.overview.welcomeBody}</p></div><button onclick={dismiss} aria-label={strings.overview.dismissWelcome}>×</button></section>
<style>
  .welcome { display:flex; align-items:flex-start; justify-content:space-between; gap:var(--space-10); padding:var(--space-12); border:var(--line-1) solid var(--color-cyan-40); border-radius:var(--radius-8); background:linear-gradient(135deg,var(--color-cyan-30),var(--color-card)); }
  h2 { margin:0; font-size:var(--type-emphasis); } p { margin:var(--space-5) 0 0; color:var(--color-text-secondary); font-size:var(--type-body-rounded); }
  button { border:0; color:var(--color-text-secondary); background:transparent; font-size:var(--type-placeholder-icon); cursor:pointer; }
</style>
