export function releaseSummary(body?: string): string | null {
  return body?.split("\n").map((line)=>line.trim()).find((line)=>
    line.length > 0 && !line.startsWith("#") && !line.startsWith("**Release:")
      && !line.startsWith("**Date:") && !line.startsWith("**Previous")
  ) ?? null;
}

export function compactNumber(value:number):string {
  return new Intl.NumberFormat(undefined,{ notation:"compact", maximumFractionDigits:1 }).format(value);
}

export function capitalized(value:string):string {
  return value.toLocaleLowerCase().replace(/\b\p{L}/gu,(character)=>character.toLocaleUpperCase());
}

export function validatedReleaseUrl(value?:string):string|null {
  if (!value) return null;
  try {
    const url=new URL(value);
    return url.protocol==="https:" && url.hostname==="github.com"
      && url.pathname.startsWith("/levy-street/world-of-claudecraft/releases/") ? url.href : null;
  } catch { return null; }
}

export function relativeDate(iso:string, nowMs:number):string {
  const seconds=Math.round((Date.parse(iso)-nowMs)/1000);
  const abs=Math.abs(seconds);
  const [amount,unit]:[number,Intl.RelativeTimeFormatUnit]=abs<3600?[Math.round(seconds/60),"minute"]:abs<86400?[Math.round(seconds/3600),"hour"]:[Math.round(seconds/86400),"day"];
  return new Intl.RelativeTimeFormat(undefined,{numeric:"auto"}).format(amount,unit);
}

export function relativeUpdated(lastSuccessMs:number|null,nowMs:number):string {
  if(lastSuccessMs===null)return strings.community.showingCached;
  const seconds=Math.max(0,Math.floor((nowMs-lastSuccessMs)/1000));
  if(seconds<2)return strings.community.updatedJustNow;
  if(seconds<60)return strings.community.updatedSeconds(seconds);
  const minutes=Math.floor(seconds/60);
  return minutes<60 ? strings.community.updatedMinutes(minutes) : strings.community.updatedHours(Math.floor(minutes/60));
}
import { strings } from "../strings";
