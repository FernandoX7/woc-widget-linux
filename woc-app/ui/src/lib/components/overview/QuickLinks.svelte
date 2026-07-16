<script lang="ts">
  import { strings } from "../../strings";
  let { openUrl }: { openUrl: (url: string) => void } = $props();
  const links = [
    [strings.overview.links.play, "https://worldofclaudecraft.com/"],
    [strings.overview.links.wiki, "https://worldofclaudecraft.com/wiki/"],
    [strings.overview.links.highScores, "https://worldofclaudecraft.com/#highscores"],
  ] as const;
</script>
<nav aria-label={strings.overview.quickLinks}>{#each links as [label,url]}<button onclick={()=>openUrl(url)}>{label} <span aria-hidden="true">↗</span></button>{/each}</nav>
<style>nav{display:grid;grid-template-columns:repeat(3,1fr);gap:var(--space-8)}button{padding:var(--space-8);border:var(--line-1) solid var(--color-card-stroke);border-radius:var(--radius-7);color:var(--color-text-secondary);background:var(--color-card);font:inherit;font-size:var(--type-button-label);font-weight:var(--weight-button-label);cursor:pointer}button:hover{color:var(--color-cyan);border-color:var(--color-cyan-40)}</style>
