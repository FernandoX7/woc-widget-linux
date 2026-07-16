<script lang="ts">
  import type { CommunityFeed, CommunityFeedName, CommunityLeader, CommunitySnapshot } from "../../ipc/types";
  import { strings } from "../../strings";
  import { capitalized, compactNumber, relativeDate, relativeUpdated, releaseSummary, validatedReleaseUrl } from "../../community/format";
  let { community, snapshotTimeMs, retry, openUrl, configureReleaseAlert }: { community:CommunitySnapshot; snapshotTimeMs:number; retry:(feed:CommunityFeedName)=>void; openUrl:(url:string)=>void; configureReleaseAlert:()=>void }=$props();
  let recentOpen=$state(false);
  const releases=$derived(community.releases.value ?? []);
  const leaders=$derived((community.leaderboard.value ?? []).slice(0,3));
  const realmName=$derived(community.currentRealm ?? community.projectStats.value?.realm ?? "—");
  const realmContext=$derived((community.realms.value?.length ?? 0)>1 ? String(community.realms.value?.length) : community.realms.value?.find((realm)=>realm.name===realmName)?.type ?? community.realms.value?.[0]?.type ?? "—");
  const leaderDetail=(entry:CommunityLeader) => [entry.cls ? capitalized(entry.cls) : null, (entry.virtualLevel ?? entry.level) !== undefined ? strings.community.level((entry.virtualLevel ?? entry.level)!) : null, entry.lifetimeXp !== undefined ? strings.community.xp(compactNumber(entry.lifetimeXp)) : null, entry.prestigeRank && entry.prestigeRank>0 ? strings.community.prestige(entry.prestigeRank) : null].filter(Boolean).join(" · ");
  const cachedMessage=(feed:CommunityFeed<unknown>) => relativeUpdated(feed.lastSuccessMs,snapshotTimeMs);
</script>

{#snippet notice(feed:CommunityFeed<unknown>,name:CommunityFeedName)}
  {#if feed.phase==="loading" && feed.value===null}
    <div class="notice loading" aria-label={strings.community.loading}><span aria-hidden="true">◌</span><span>{strings.community.loading}</span></div>
  {:else if feed.phase==="cached" || feed.phase==="unavailable"}
    <div class:failed={feed.phase==="unavailable"} class="notice"><span aria-hidden="true">⚠</span><span>{feed.phase==="cached" ? cachedMessage(feed) : strings.community.unavailable}</span><button onclick={()=>retry(name)}>{strings.community.retry}</button></div>
  {/if}
{/snippet}
{#snippet skeleton()}<div class="skeleton" aria-label={strings.community.loading}>{#each [240,190,240] as width}<i style={`width:${width}px`}></i>{/each}</div>{/snippet}
{#snippet title(icon:string,text:string)}<h2><span aria-hidden="true">{icon}</span>{text}</h2>{/snippet}

<section class="community-page" aria-label={strings.community.pageLabel}>
  <article class="card snapshot">
    {@render title("◉",strings.community.around("Claudemoon"))}
    <div class="metrics">
      <div><strong>{community.projectStats.value?.accountsCreated!==undefined ? compactNumber(community.projectStats.value.accountsCreated) : "—"}</strong><small>{strings.community.accounts}</small></div><b></b>
      <div><strong>{realmName}</strong><small>{strings.community.realm}</small></div><b></b>
      <div><strong>{realmContext}</strong><small>{(community.realms.value?.length ?? 0)>1 ? strings.community.realms : strings.community.realmType}</small></div>
    </div>
    {@render notice(community.projectStats,"projectStats")}
    {#if community.realms.phase==="cached" || community.realms.phase==="unavailable"}{@render notice(community.realms,"realms")}{/if}
  </article>
  <article class="card release">
    <header>{@render title("✦",strings.community.latestRelease)}<button class="alert" onclick={configureReleaseAlert} title={strings.community.releaseAlert} aria-label={strings.community.releaseAlert}>◉</button>{#if releases[0]?.tag}<span class="tag">{releases[0].tag}</span>{/if}</header>
    {#if releases[0]}
      <strong class="release-name">{releases[0].name ?? releases[0].tag ?? "—"}</strong>
      {#if releaseSummary(releases[0].body)}<p>{releaseSummary(releases[0].body)}</p>{/if}
      {#if releases[0].publishedAt}<time>{relativeDate(releases[0].publishedAt,snapshotTimeMs)}</time>{/if}
      {#if validatedReleaseUrl(releases[0].url)}<button class="view" onclick={()=>openUrl(validatedReleaseUrl(releases[0].url)!)}>{strings.community.viewRelease} ↗</button>{/if}
      {#if releases.length>1}<button class="disclosure" aria-expanded={recentOpen} onclick={()=>recentOpen=!recentOpen}>{strings.community.recentReleases}<span>{recentOpen?"⌃":"⌄"}</span></button>{/if}
      {#if recentOpen}<div class="recent">{#each releases.slice(1,3) as item}<button disabled={!validatedReleaseUrl(item.url)} onclick={()=>validatedReleaseUrl(item.url)&&openUrl(validatedReleaseUrl(item.url)!)}><em>{item.tag??"—"}</em><span>{item.name??item.tag??"—"}</span>{#if validatedReleaseUrl(item.url)}↗{/if}</button>{/each}</div>{/if}
      {@render notice(community.releases,"releases")}
    {:else if community.releases.phase==="idle" || community.releases.phase==="loading"}{@render skeleton()}
    {:else if community.releases.phase==="live"}<p>{strings.community.noReleases}</p>{:else}{@render notice(community.releases,"releases")}{/if}
  </article>
  <article class="card leaderboard">
    {@render title("★",strings.community.leaderboard)}
    {#each leaders as leader,index}<div class="leader"><strong class:gold={(leader.rank??index+1)===1}>#{leader.rank??index+1}</strong><span><b>{leader.name??"—"}</b><small>{leaderDetail(leader)}</small></span></div>{/each}
    {#if leaders.length===0}{#if community.leaderboard.phase==="idle" || community.leaderboard.phase==="loading"}{@render skeleton()}{:else if community.leaderboard.phase==="live"}<p>{strings.community.noRankings}</p>{:else}{@render notice(community.leaderboard,"leaderboard")}{/if}{:else}{@render notice(community.leaderboard,"leaderboard")}{/if}
  </article>
  <nav aria-label="Community links">{#each Object.entries(strings.community.links) as [key,label]}<button class:prominent={key==="discord"} onclick={()=>openUrl(strings.community.urls[key as keyof typeof strings.community.urls])}>{label}</button>{/each}</nav>
</section>

<style>
  .community-page{flex:1;min-height:0;overflow-y:auto;padding:12px 16px;display:flex;flex-direction:column;gap:12px}.card{padding:12px;border:1px solid var(--color-card-stroke);border-radius:var(--radius-14);background:var(--color-card);backdrop-filter:blur(var(--glass-blur))}.card h2{margin:0;display:flex;gap:6px;align-items:center;font-size:var(--type-section-label);letter-spacing:var(--tracking-08);color:var(--color-text-secondary)}.metrics{display:grid;grid-template-columns:1fr 1px 1fr 1px 1fr;align-items:center;margin-top:10px}.metrics>div{text-align:center;min-width:0}.metrics strong,.metrics small{display:block;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}.metrics strong{font:var(--weight-stat-value) var(--type-stat-value) var(--font-rounded)}.metrics div:first-child strong{color:var(--color-cyan)}.metrics div:nth-of-type(2) strong{color:var(--color-green)}.metrics div:nth-of-type(3) strong{color:var(--color-gold)}.metrics small{margin-top:2px;font-size:var(--type-stat-label);font-weight:var(--weight-stat-label);color:var(--color-text-tertiary)}.metrics>b{height:22px;background:var(--color-card-stroke)}header{display:flex;align-items:center;gap:6px}header h2{flex:1}.alert{border:0;background:transparent;color:var(--color-cyan);cursor:pointer}.tag{padding:2px 7px;border-radius:999px;background:rgb(20% 90% 95% / 13%);color:var(--color-cyan);font-size:var(--type-pill)}.release-name{display:block;margin-top:8px;font-size:var(--type-emphasis)}p{margin:6px 0;color:var(--color-text-secondary);font-size:var(--type-body-rounded)}time{display:block;color:var(--color-text-tertiary);font-size:var(--type-timestamp)}button{font-family:inherit}.view{width:100%;margin-top:8px;padding:7px;border:1px solid var(--color-cyan-30);border-radius:999px;background:transparent;color:var(--color-cyan);font-weight:600;cursor:pointer}.disclosure{width:100%;display:flex;justify-content:space-between;margin-top:8px;padding:8px 0 0;border:0;border-top:1px solid var(--color-card-stroke);background:none;color:var(--color-text-secondary);cursor:pointer}.recent button{display:flex;width:100%;gap:8px;padding:6px 0;border:0;background:none;color:var(--color-text-secondary);text-align:left}.recent em{color:var(--color-cyan);font-style:normal}.recent span{flex:1}.leader{display:flex;gap:8px;align-items:center;margin-top:9px}.leader>strong{width:22px;color:var(--color-text-secondary)}.leader>strong.gold{color:var(--color-gold)}.leader span{min-width:0}.leader b,.leader small{display:block}.leader b{font-size:var(--type-row-label)}.leader small{margin-top:2px;color:var(--color-text-secondary);font-size:var(--type-placeholder-hint)}.notice{display:flex;align-items:center;gap:6px;margin-top:8px;color:var(--color-gold);font-size:var(--type-placeholder-hint)}.notice.failed{color:var(--color-red)}.notice span:nth-child(2){flex:1;color:var(--color-text-secondary)}.notice button{padding:4px 7px;border:1px solid var(--color-cyan-30);border-radius:999px;background:none;color:var(--color-cyan);cursor:pointer}.skeleton{display:flex;flex-direction:column;gap:8px;margin-top:8px}.skeleton i{max-width:100%;height:12px;border-radius:7px;background:var(--color-card-stroke)}nav{display:grid;grid-template-columns:repeat(4,1fr);gap:8px}nav button{padding:8px 3px;border:1px solid var(--color-card-stroke);border-radius:999px;background:var(--color-card);color:var(--color-text-secondary);font-size:var(--type-button-label);cursor:pointer}nav button.prominent{border-color:var(--color-cyan-30);color:var(--color-cyan)}
</style>
