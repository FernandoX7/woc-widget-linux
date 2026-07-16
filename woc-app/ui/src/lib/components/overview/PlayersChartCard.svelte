<script module lang="ts">
  let nextGradientId = 0;
</script>

<script lang="ts">
  import {
    AreaSeries,
    ColorType,
    CrosshairMode,
    LineStyle,
    createChart,
    type AreaData,
    type ChartOptions,
    type DeepPartial,
    type IChartApi,
    type ISeriesApi,
    type MouseEventParams,
    type Time,
  } from 'lightweight-charts';
  import { strings } from '../../strings';
  import type { ChartRange, PlayerChartAnalytics, PlayerChartPoint } from '../../overview/types';

  let {
    analytics,
    disabled = false,
    onRangeChange,
  }: {
    analytics: PlayerChartAnalytics;
    disabled?: boolean;
    onRangeChange?: (range: ChartRange) => void;
  } = $props();

  let host = $state<HTMLDivElement>();
  let chart = $state<IChartApi | null>(null);
  let seriesList: Array<{ series: ISeriesApi<'Area'>; glow: ISeriesApi<'Area'> | null; points: PlayerChartPoint[] }> = [];
  let selectedIndex = $state<number | null>(null);
  let tooltipPoint = $state<PlayerChartPoint | null>(null);
  let tooltipX = $state(0);
  let tooltipY = $state(0);
  const gradientId = `player-area-${++nextGradientId}`;

  const ranges: ChartRange[] = ['1h', '6h', '24h', '7d'];
  const observed = $derived(analytics.points.filter((point) => point.value !== null));

  function asTime(seconds: number): Time {
    return seconds as Time;
  }

  function segments(): PlayerChartPoint[][] {
    const result: PlayerChartPoint[][] = [];
    for (let index = 0; index < analytics.points.length; index += 1) {
      const point = analytics.points[index];
      if (point.value === null) continue;
      const previous = analytics.points[index - 1];
      if (!result.length || previous?.value === null) result.push([]);
      result.at(-1)?.push(point);
    }
    return result;
  }

  function formatTime(seconds: number): string {
    const date = new Date(seconds * 1000);
    const month = new Intl.DateTimeFormat('en-US', { month: 'short' }).format(date);
    const day = new Intl.DateTimeFormat('en-US', { day: 'numeric' }).format(date);
    const time = new Intl.DateTimeFormat('en-US', {
      hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    }).format(date);
    return `${month} ${day} ${time}`;
  }

  function visualX(time: number): number {
    const first = observed[0]?.time ?? time;
    const last = observed.at(-1)?.time ?? time;
    return last === first ? 500 : ((time - first) / (last - first)) * 1000;
  }

  function visualY(value: number): number {
    const values = observed.map((point) => point.value!);
    const minimum = analytics.yDomain?.min ?? Math.min(...values);
    const maximum = analytics.yDomain?.max ?? Math.max(...values);
    return maximum === minimum ? 40 : 4 + ((maximum - value) / (maximum - minimum)) * 72;
  }

  function linePath(points: PlayerChartPoint[]): string {
    return points.map((point, index) => `${index === 0 ? 'M' : 'L'}${visualX(point.time)},${visualY(point.value!)}`).join(' ');
  }

  function areaPath(points: PlayerChartPoint[]): string {
    if (points.length < 2) return '';
    return `${linePath(points)} L${visualX(points.at(-1)!.time)},78 L${visualX(points[0].time)},78 Z`;
  }

  function axisTime(seconds: number): string {
    return new Intl.DateTimeFormat('en-US', { hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(new Date(seconds * 1000));
  }

  function alignTooltip(point: PlayerChartPoint) {
    if (!host || point.value === null) return;
    tooltipX = visualX(point.time) / 1000 * host.clientWidth;
    tooltipY = visualY(point.value) / 104 * host.clientHeight;
  }

  function showPoint(point: PlayerChartPoint, index: number | null = null) {
    if (!chart || point.value === null) return;
    const owner = seriesList.find((entry) => entry.points.includes(point));
    if (!owner) return;
    selectedIndex = index;
    tooltipPoint = point;
    alignTooltip(point);
    chart.setCrosshairPosition(point.value, asTime(point.time), owner.series);
  }

  function clearSelection() {
    selectedIndex = null;
    tooltipPoint = null;
    chart?.clearCrosshairPosition();
  }

  function handleCrosshair(param: MouseEventParams<Time>) {
    if (!param.time || !param.point) {
      if (selectedIndex === null) tooltipPoint = null;
      return;
    }
    const entry = seriesList.find(({ series }) => {
      const datum = param.seriesData.get(series);
      return datum && 'value' in datum;
    });
    if (!entry) {
      if (selectedIndex === null) tooltipPoint = null;
      return;
    }
    const seconds = Number(param.time);
    const point = entry.points.find((candidate) => candidate.time === seconds);
    if (!point) return;
    selectedIndex = null;
    tooltipPoint = point;
    alignTooltip(point);
  }

  function inspect(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      clearSelection();
      return;
    }
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    if (!observed.length) return;
    event.preventDefault();
    const delta = event.key === 'ArrowRight' ? 1 : -1;
    const initial = event.key === 'ArrowRight' ? 0 : observed.length - 1;
    const next = selectedIndex === null ? initial : Math.max(0, Math.min(observed.length - 1, selectedIndex + delta));
    showPoint(observed[next], next);
  }

  function render() {
    if (!chart) return;
    for (const entry of seriesList) {
      chart.removeSeries(entry.series);
      if (entry.glow) chart.removeSeries(entry.glow);
    }
    seriesList = segments().map((points) => {
      const isolatedSegment = points.length === 1;
      const glow = isolatedSegment ? null : chart!.addSeries(AreaSeries, {
        lineColor: 'rgba(51,230,242,.38)', lineWidth: 4,
        topColor: 'transparent', bottomColor: 'transparent',
        priceLineVisible: false, lastValueVisible: false, crosshairMarkerVisible: false,
        autoscaleInfoProvider: () => analytics.yDomain ? { priceRange: { minValue: analytics.yDomain.min, maxValue: analytics.yDomain.max } } : null,
      });
      const series = chart!.addSeries(AreaSeries, {
        lineColor: isolatedSegment ? 'transparent' : '#33e6f2', lineWidth: 2,
        topColor: isolatedSegment ? 'transparent' : 'rgba(51,230,242,.34)',
        bottomColor: isolatedSegment ? 'transparent' : 'rgba(153,77,242,.02)',
        priceLineVisible: false, lastValueVisible: false, crosshairMarkerVisible: true,
        crosshairMarkerRadius: 4, crosshairMarkerBackgroundColor: '#33e6f2', crosshairMarkerBorderColor: '#ffffff',
        autoscaleInfoProvider: () => analytics.yDomain ? { priceRange: { minValue: analytics.yDomain.min, maxValue: analytics.yDomain.max } } : null,
      });
      const data = points.map((point) => ({ time: asTime(point.time), value: point.value! })) as AreaData<Time>[];
      glow?.setData(data);
      series.setData(data);
      return { series, glow, points };
    });
    chart.timeScale().fitContent();
    clearSelection();
  }

  $effect(() => {
    if (!host) return;
    const mountedHost = host;
    let mountedChart: IChartApi | null = null;
    const options: DeepPartial<ChartOptions> = {
      autoSize: true,
      height: 104,
      layout: { background: { type: ColorType.Solid, color: 'transparent' }, textColor: 'rgba(255,255,255,.5)', fontSize: 10, attributionLogo: false },
      grid: { vertLines: { visible: false }, horzLines: { color: 'rgba(255,255,255,.13)' } },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: 'rgba(255,255,255,.68)', style: LineStyle.Dashed, width: 1, labelVisible: false },
        horzLine: { visible: false, labelVisible: false },
      },
      rightPriceScale: { visible: false, borderVisible: false },
      leftPriceScale: { visible: false },
      timeScale: { borderVisible: false, timeVisible: true, secondsVisible: false, rightOffset: 1 },
      handleScroll: false,
      handleScale: false,
    };
    const frame = requestAnimationFrame(() => {
      mountedChart = createChart(mountedHost, options);
      mountedChart.subscribeCrosshairMove(handleCrosshair);
      chart = mountedChart;
    });
    return () => {
      cancelAnimationFrame(frame);
      mountedChart?.unsubscribeCrosshairMove(handleCrosshair);
      mountedChart?.remove();
      if (chart === mountedChart) chart = null;
      seriesList = [];
    };
  });

  $effect(() => {
    analytics;
    render();
  });
</script>

<section class="chart-card" aria-labelledby="players-chart-title">
  <header>
    <div>
      <h2 id="players-chart-title">{strings.overview.chartTitle}</h2>
      <span class="resolution">{analytics.resolutionLabel}</span>
    </div>
    <span class="coverage">{strings.overview.coverage(Math.round(analytics.coveragePercent))}</span>
  </header>

  <div class="range-picker" aria-label={strings.overview.rangePicker}>
    {#each ranges as range}
      <button
        type="button"
        class:active={analytics.range === range}
        aria-pressed={analytics.range === range}
        {disabled}
        onclick={() => onRangeChange?.(range)}
      >{range}</button>
    {/each}
  </div>

  {#if observed.length === 0}
    <div class="empty" role="status">
      <span aria-hidden="true">⌁</span>
      <strong>{disabled ? strings.overview.loadingHistory : strings.overview.emptyHistory}</strong>
      <small>{strings.overview.emptyHistoryHint}</small>
    </div>
  {:else}
    <button
      type="button"
      class="plot"
      data-segment-count={segments().length}
      data-isolated-count={observed.filter((point) => point.isolated).length}
      aria-label={analytics.accessibleDescription ?? strings.overview.chartAccessibility}
      onkeydown={inspect}
      onmouseleave={() => selectedIndex === null && clearSelection()}
    >
      <div class="chart" bind:this={host}></div>
      <svg class="visual-series" viewBox="0 0 1000 104" preserveAspectRatio="none" aria-hidden="true">
        <defs>
          <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stop-color="#33e6f2" stop-opacity=".34" />
            <stop offset="1" stop-color="#994df2" stop-opacity=".02" />
          </linearGradient>
        </defs>
        <line class="grid-line" x1="0" y1="78" x2="1000" y2="78" />
        {#each segments() as segment}
          {#if segment.length > 1}
            <path class="area" d={areaPath(segment)} fill={`url(#${gradientId})`} />
            <path class="glow" d={linePath(segment)} />
            <path class="line" d={linePath(segment)} />
          {:else}
            <circle class="isolated-dot" cx={visualX(segment[0].time)} cy={visualY(segment[0].value!)} r="5" />
          {/if}
        {/each}
        {#if observed.length > 1}
          <text x="0" y="101" text-anchor="start">{axisTime(observed[0].time)}</text>
          <text x="500" y="101" text-anchor="middle">{axisTime((observed[0].time + observed.at(-1)!.time) / 2)}</text>
          <text x="1000" y="101" text-anchor="end">{axisTime(observed.at(-1)!.time)}</text>
        {/if}
        {#if tooltipPoint?.value !== null && tooltipPoint !== null}
          <line class="selection-line" x1={visualX(tooltipPoint.time)} y1="0" x2={visualX(tooltipPoint.time)} y2="78" />
        {/if}
      </svg>
      {#if tooltipPoint?.value !== null && tooltipPoint !== null}
        <div class="tooltip" style={`left:${tooltipX}px; top:${tooltipY}px`} aria-live="polite">
          <strong>{strings.overview.playerTooltip(tooltipPoint.value)}</strong>
          <span>{formatTime(tooltipPoint.time)}</span>
        </div>
      {/if}
    </button>
  {/if}
</section>

<style>
  .chart-card { position: relative; }
  header { display: flex; align-items: flex-start; justify-content: space-between; }
  header > div { display: flex; align-items: baseline; gap: 7px; }
  h2 { margin: 0; color: var(--color-text-secondary); font-size: var(--type-section-label); font-weight: var(--weight-section-label); letter-spacing: var(--tracking-16); }
  .resolution { color: var(--color-text-tertiary); font-size: var(--type-axis); }
  .coverage { padding: 3px 6px; border-radius: var(--pill-radius); background: var(--color-cyan-30); color: var(--color-cyan); font-size: var(--type-badge-label); font-weight: var(--weight-badge-label); }
  .range-picker { display: flex; gap: 2px; width: fit-content; margin: 7px 0 2px auto; padding: 2px; border-radius: var(--radius-7); background: rgb(255 255 255 / 6%); }
  button { min-width: 34px; padding: 3px 6px; border: 0; border-radius: 5px; background: transparent; color: var(--color-text-tertiary); font-size: var(--type-pill); cursor: pointer; }
  button.active { background: var(--color-switcher-selection); color: var(--color-text-primary); font-weight: var(--weight-semibold); }
  button:focus-visible, .plot:focus-visible { outline: 2px solid var(--color-cyan); outline-offset: 2px; }
  .plot { position: relative; display: block; width: 100%; height: var(--chart-height); min-width: 0; padding: 0; border: 0; border-radius: 0; background: transparent; outline: none; cursor: crosshair; text-align: initial; }
  .chart { height: 100%; opacity: 0; }
  .visual-series { position:absolute; inset:0; width:100%; height:100%; pointer-events:none; overflow:visible; }
  .visual-series .grid-line { stroke:rgb(255 255 255 / 13%); stroke-width:1; vector-effect:non-scaling-stroke; }
  .visual-series .glow { fill:none; stroke:rgb(51 230 242 / 38%); stroke-width:6; filter:blur(3px); vector-effect:non-scaling-stroke; }
  .visual-series .line { fill:none; stroke:#33e6f2; stroke-width:2; stroke-linejoin:round; stroke-linecap:round; vector-effect:non-scaling-stroke; }
  .visual-series .isolated-dot { fill:#33e6f2; stroke:#fff; stroke-width:1; vector-effect:non-scaling-stroke; filter:drop-shadow(0 0 3px rgb(51 230 242 / 65%)); }
  .visual-series text { fill:rgb(255 255 255 / 50%); font-size:9px; font-family:var(--font-rounded); }
  .visual-series .selection-line { stroke:rgb(255 255 255 / 68%); stroke-width:1; stroke-dasharray:4 4; vector-effect:non-scaling-stroke; }
  .tooltip { position: absolute; z-index: 2; display: grid; gap: 1px; min-width: 82px; padding: 5px 7px; border: 1px solid rgb(255 255 255 / 15%); border-radius: var(--radius-7); background: var(--color-chart-annotation-bg); box-shadow: 0 2px 5px #0008; pointer-events: none; transform: translate(-50%, calc(-100% - 8px)); font-size: var(--type-annotation-time); }
  .tooltip strong { color: var(--color-text-primary); font-size: var(--type-pill); }
  .tooltip span { color: var(--color-text-secondary); white-space: nowrap; }
  .empty { height: calc(var(--chart-height) + 27px); display: grid; place-content: center; justify-items: center; gap: 3px; color: var(--color-text-tertiary); text-align: center; }
  .empty > span { font-size: 22px; color: var(--color-cyan); }
  .empty strong { color: var(--color-text-secondary); font-size: var(--type-emphasis); }
  .empty small { font-size: var(--type-placeholder-hint); }
  @media (prefers-contrast: more) { .chart-card { background: var(--color-card-opaque); border-color: var(--color-card-stroke-strong); } }
  @media (prefers-reduced-transparency: reduce) { .chart-card { background: var(--color-card-opaque); } }
</style>
