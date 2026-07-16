<script lang="ts">
  import { onMount } from 'svelte';
  import {
    CandlestickSeries,
    ColorType,
    CrosshairMode,
    LineStyle,
    createChart,
    type CandlestickData,
    type IChartApi,
    type IPriceLine,
    type ISeriesApi,
    type MouseEventParams,
    type Time,
  } from 'lightweight-charts';
  import { signedPercent } from '../../format';
  import { candleDomain, chartPrice } from '../../market/chart';
  import type { CandleInterval, MarketCandle, MarketCandlestickCardProps } from '../../market/types';

  let {
    candles,
    selectedInterval,
    loadedInterval,
    feedState,
    refreshing = false,
    onIntervalChange,
  }: MarketCandlestickCardProps = $props();

  const intervals: CandleInterval[] = ['1m', '5m', '15m', '1h', '4h'];
  let host = $state<HTMLDivElement>();
  let chart: IChartApi | null = null;
  let series: ISeriesApi<'Candlestick'> | null = null;
  let latestLine: IPriceLine | null = null;
  let inspectedIndex = $state<number | null>(null);
  let pointerCandle = $state<MarketCandle | null>(null);
  let closeChipY = $state(0);

  const visibleCandles = $derived.by(() => {
    if (selectedInterval !== loadedInterval) return [];
    return candles
      .filter((candle) => Number.isFinite(candle.time) && candle.time > 0
        && [candle.open, candle.high, candle.low, candle.close, candle.volume].every(Number.isFinite))
      .sort((a, b) => a.time - b.time)
      .slice(-60);
  });
  const displayedCandle = $derived(pointerCandle ?? (inspectedIndex === null
    ? visibleCandles.at(-1) ?? null
    : visibleCandles[inspectedIndex] ?? null));
  const isInspecting = $derived(pointerCandle !== null || inspectedIndex !== null);
  const emptyState = $derived(refreshing || feedState === 'loading'
    ? 'loading'
    : feedState === 'unavailable' || selectedInterval !== loadedInterval
      ? 'unavailable'
      : 'gathering');
  const visualDomain = $derived(candleDomain(visibleCandles));

  function asTime(seconds: number): Time { return seconds as Time; }

  function visualX(time: number): number {
    const first = visibleCandles[0]?.time ?? time;
    const last = visibleCandles.at(-1)?.time ?? time;
    return last === first ? 465 : 8 + ((time - first) / (last - first)) * 914;
  }

  function visualY(price: number): number {
    if (!visualDomain || visualDomain.max === visualDomain.min) return 40;
    return 4 + ((visualDomain.max - price) / (visualDomain.max - visualDomain.min)) * 74;
  }

  function candleWidth(): number {
    if (visibleCandles.length < 2) return 6;
    return Math.max(2, Math.min(9, (914 / visibleCandles.length) * 0.62));
  }

  function axisTime(seconds: number): string {
    return new Intl.DateTimeFormat('en-US', {
      hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    }).format(new Date(seconds * 1000));
  }

  function formatDate(seconds: number): string {
    return new Intl.DateTimeFormat('en-US', {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hourCycle: 'h23',
    }).format(new Date(seconds * 1000));
  }

  function signedChange(candle: MarketCandle): string {
    const change = candle.open === 0 ? 0 : ((candle.close - candle.open) / candle.open) * 100;
    return signedPercent(change);
  }

  function candleDescription(candle: MarketCandle): string {
    const direction = candle.close >= candle.open ? 'rising' : 'falling';
    return `${formatDate(candle.time)}, ${direction} ${signedChange(candle)}, open ${chartPrice(candle.open)}, high ${chartPrice(candle.high)}, low ${chartPrice(candle.low)}, close ${chartPrice(candle.close)}`;
  }

  function selectCandle(index: number) {
    if (!series || !chart || !visibleCandles[index]) return;
    inspectedIndex = index;
    pointerCandle = null;
    const candle = visibleCandles[index];
    chart.setCrosshairPosition(candle.close, asTime(candle.time), series);
  }

  function clearInspection() {
    inspectedIndex = null;
    pointerCandle = null;
    chart?.clearCrosshairPosition();
  }

  function inspect(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (isInspecting) event.preventDefault();
      clearInspection();
      return;
    }
    if ((event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') || !visibleCandles.length) return;
    event.preventDefault();
    const delta = event.key === 'ArrowRight' ? 1 : -1;
    const initial = delta > 0 ? 0 : visibleCandles.length - 1;
    selectCandle(inspectedIndex === null ? initial : Math.max(0, Math.min(visibleCandles.length - 1, inspectedIndex + delta)));
  }

  function handleCrosshair(param: MouseEventParams<Time>) {
    if (inspectedIndex !== null) return;
    const datum = series ? param.seriesData.get(series) : null;
    if (!datum || !('open' in datum)) {
      pointerCandle = null;
      return;
    }
    pointerCandle = visibleCandles.find((candle) => candle.time === Number(param.time)) ?? null;
  }

  function render() {
    if (!chart || !series) return;
    const domain = candleDomain(visibleCandles);
    series.applyOptions({
      autoscaleInfoProvider: () => domain ? { priceRange: { minValue: domain.min, maxValue: domain.max } } : null,
    });
    series.setData(visibleCandles.map((candle) => ({
      time: asTime(candle.time), open: candle.open, high: candle.high, low: candle.low, close: candle.close,
    })) as CandlestickData<Time>[]);
    if (latestLine) {
      series.removePriceLine(latestLine);
      latestLine = null;
    }
    const latest = visibleCandles.at(-1);
    if (latest) {
      latestLine = series.createPriceLine({
        price: latest.close, color: latest.close >= latest.open ? 'rgba(84,219,125,.8)' : 'rgba(245,107,107,.8)',
        lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: false, title: '',
      });
      closeChipY = visualY(latest.close);
    }
    chart.timeScale().fitContent();
    clearInspection();
  }

  onMount(() => {
    if (!host) return;
    chart = createChart(host, {
      autoSize: true,
      height: 104,
      layout: { background: { type: ColorType.Solid, color: 'transparent' }, textColor: 'rgba(255,255,255,.68)', fontSize: 10, attributionLogo: false },
      grid: { vertLines: { visible: false }, horzLines: { color: 'rgba(255,255,255,.13)' } },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: 'rgba(255,255,255,.68)', style: LineStyle.Dashed, width: 1, labelVisible: false },
        horzLine: { visible: false, labelVisible: false },
      },
      rightPriceScale: { visible: true, borderVisible: false, minimumWidth: 58 },
      leftPriceScale: { visible: false },
      timeScale: { borderVisible: false, timeVisible: true, secondsVisible: false, rightOffset: 1 },
      handleScroll: false,
      handleScale: false,
      localization: { priceFormatter: chartPrice },
    });
    series = chart.addSeries(CandlestickSeries, {
      upColor: '#54db7d', downColor: 'transparent',
      borderVisible: true, borderUpColor: '#54db7d', borderDownColor: '#f56b6b',
      wickUpColor: '#54db7d', wickDownColor: '#f56b6b',
      priceLineVisible: false, lastValueVisible: false,
    });
    chart.subscribeCrosshairMove(handleCrosshair);
    render();
    return () => {
      chart?.unsubscribeCrosshairMove(handleCrosshair);
      chart?.remove();
      chart = null;
      series = null;
      latestLine = null;
    };
  });

  $effect(() => { visibleCandles; selectedInterval; render(); });
</script>

<section class="card" aria-labelledby="market-candle-title">
  <h2 id="market-candle-title">$WOC PRICE</h2>
  <div class="picker-row">
    <span id="candle-width-label">CANDLE</span>
    <div class="picker" role="group" aria-labelledby="candle-width-label">
      {#each intervals as interval}
        <button type="button" class:active={selectedInterval === interval} aria-pressed={selectedInterval === interval}
          onclick={() => interval !== selectedInterval && onIntervalChange?.(interval)}>{interval}</button>
      {/each}
    </div>
  </div>

  {#if displayedCandle}
    <div class:falling={displayedCandle.close < displayedCandle.open} class="detail" aria-live="polite" aria-label={candleDescription(displayedCandle)}>
      <div><strong>{isInspecting ? '⌖ SELECTED' : '◷ LATEST'}</strong><span>{formatDate(displayedCandle.time)}</span><b>{signedChange(displayedCandle)}</b></div>
      <dl>
        <div><dt>O</dt><dd>{chartPrice(displayedCandle.open)}</dd></div>
        <div><dt>H</dt><dd>{chartPrice(displayedCandle.high)}</dd></div>
        <div><dt>L</dt><dd>{chartPrice(displayedCandle.low)}</dd></div>
        <div><dt>C</dt><dd>{chartPrice(displayedCandle.close)}</dd></div>
      </dl>
    </div>
  {/if}

  <button type="button" class:hidden={visibleCandles.length < 2} class="plot" aria-label={`$WOC candlestick chart, ${visibleCandles.length} ${selectedInterval} candles. Use left and right arrows to inspect candles; Escape returns to latest.`}
      aria-describedby="market-candle-data" onkeydown={inspect} onmouseleave={() => inspectedIndex === null && (pointerCandle = null)}>
      <div class="chart" bind:this={host}></div>
      <svg class="visual-candles" viewBox="0 0 1000 104" preserveAspectRatio="none" aria-hidden="true">
        <line class="grid-line" x1="0" y1="78" x2="1000" y2="78" />
        {#each visibleCandles as candle}
          {@const rising = candle.close >= candle.open}
          {@const bodyTop = visualY(Math.max(candle.open, candle.close))}
          {@const bodyBottom = visualY(Math.min(candle.open, candle.close))}
          <g class:rising class:falling={!rising} class="candle">
            <line class="wick" x1={visualX(candle.time)} y1={visualY(candle.high)} x2={visualX(candle.time)} y2={visualY(candle.low)} />
            <rect class="body" x={visualX(candle.time) - candleWidth() / 2} y={bodyTop}
              width={candleWidth()} height={Math.max(1, bodyBottom - bodyTop)} />
          </g>
        {/each}
        {#if visibleCandles.at(-1)}
          <line class:falling={visibleCandles.at(-1)!.close < visibleCandles.at(-1)!.open} class="latest-line" x1="0" y1={visualY(visibleCandles.at(-1)!.close)} x2="922" y2={visualY(visibleCandles.at(-1)!.close)} />
          <text class="price-label" x="996" y={Math.max(9, visualY(visibleCandles.at(-1)!.close) + 3)} text-anchor="end">{chartPrice(visibleCandles.at(-1)!.close)}</text>
        {/if}
        {#if visibleCandles.length > 1}
          <text x="8" y="101" text-anchor="start">{axisTime(visibleCandles[0].time)}</text>
          <text x="465" y="101" text-anchor="middle">{axisTime((visibleCandles[0].time + visibleCandles.at(-1)!.time) / 2)}</text>
          <text x="922" y="101" text-anchor="end">{axisTime(visibleCandles.at(-1)!.time)}</text>
        {/if}
        {#if isInspecting && displayedCandle}
          <line class="selection-line" x1={visualX(displayedCandle.time)} y1="0" x2={visualX(displayedCandle.time)} y2="78" />
        {/if}
      </svg>
      {#if visibleCandles.at(-1) && closeChipY > 0}
        <span class:falling={visibleCandles.at(-1)!.close < visibleCandles.at(-1)!.open} class="close-chip" style={`top:${closeChipY}px`}>
          {selectedInterval} close ${chartPrice(visibleCandles.at(-1)!.close)}
        </span>
      {/if}
  </button>
  {#if visibleCandles.length < 2}
    <div class="empty" role="status">
      <span aria-hidden="true">{emptyState === 'loading' ? '◌' : emptyState === 'unavailable' ? '⌁' : '▥'}</span>
      <strong>{emptyState === 'loading' ? 'Loading chart' : emptyState === 'unavailable' ? 'Chart unavailable' : 'Gathering candles'}</strong>
      <small>{emptyState === 'loading' ? 'Fetching the latest price candles' : emptyState === 'unavailable' ? 'Try refreshing the chart feed' : 'The chart needs at least two candles'}</small>
    </div>
  {/if}
  <ol id="market-candle-data" class="sr-only">
    {#each visibleCandles as candle}<li>{candleDescription(candle)}</li>{/each}
  </ol>
</section>

<style>
  .card { padding: var(--space-12); border: 1px solid var(--color-card-stroke); border-radius: var(--radius-14); background: var(--color-card); }
  h2 { margin: 0 0 var(--space-9); color: var(--color-text-secondary); font-size: var(--type-section-label); font-weight: var(--weight-section-label); letter-spacing: var(--tracking-08); }
  .picker-row { display: flex; align-items: center; gap: var(--space-8); margin-bottom: var(--space-9); }
  .picker-row > span { width: var(--picker-label-width); color: var(--color-text-tertiary); font-size: var(--type-picker-row-label); font-weight: var(--weight-picker-row-label); letter-spacing: var(--tracking-06); }
  .picker { display: flex; flex: 1; padding: 2px; border-radius: var(--radius-7); background: rgb(0 0 0 / 18%); }
  .picker button { flex: 1; padding: 3px; border: 0; border-radius: 5px; background: transparent; color: var(--color-text-tertiary); font-size: var(--type-pill); cursor: pointer; }
  .picker button.active { background: var(--color-switcher-selection); color: var(--color-text-primary); font-weight: var(--weight-semibold); }
  button:focus-visible { outline: 2px solid var(--color-cyan); outline-offset: 2px; }
  .detail { box-sizing: border-box; min-height: 52px; margin-bottom: var(--space-6); padding: var(--space-6) var(--space-8); border: .75px solid var(--color-green); border-color: color-mix(in srgb, var(--color-green) 25%, transparent); border-radius: var(--radius-7); background: var(--color-chart-annotation-bg); }
  .detail.falling { border-color: color-mix(in srgb, var(--color-red) 25%, transparent); }
  .detail > div { display: flex; align-items: center; gap: var(--space-6); color: var(--color-text-secondary); font-size: var(--type-annotation-time); }
  .detail > div span { margin-left: auto; }.detail b { color: var(--color-green); font-size: var(--type-crypto-change); }.detail.falling b { color: var(--color-red); }
  dl { display: flex; gap: var(--space-8); margin: var(--space-4) 0 0; } dl > div { display: flex; flex: 1; gap: var(--space-2); min-width: 0; } dt { color: var(--color-text-tertiary); font-size: var(--type-stat-label); } dd { overflow: hidden; margin: 0; color: var(--color-text-primary); font: var(--type-axis) var(--font-mono-digits); text-overflow: ellipsis; }
  .plot { position: relative; display: block; width: 100%; height: 104px; padding: 0; overflow: hidden; border: 0; border-radius: var(--radius-7); background: transparent; text-align: initial; cursor: crosshair; }
  .plot.hidden { position: absolute; width: 1px; height: 1px; overflow: hidden; visibility: hidden; pointer-events: none; }
  .chart { width: 100%; height: 104px; opacity: 0; }
  .visual-candles { position: absolute; inset: 0; width: 100%; height: 100%; overflow: hidden; pointer-events: none; }
  .visual-candles .grid-line { stroke: rgb(255 255 255 / 13%); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .visual-candles .wick { stroke-width: 1; vector-effect: non-scaling-stroke; }
  .visual-candles .body { stroke-width: 1; vector-effect: non-scaling-stroke; }
  .visual-candles .rising .wick, .visual-candles .rising .body { stroke: #54db7d; fill: #54db7d; }
  .visual-candles .falling .wick, .visual-candles .falling .body { stroke: #f56b6b; fill: transparent; }
  .visual-candles .latest-line { stroke: rgb(84 219 125 / 80%); stroke-width: 1; stroke-dasharray: 4 4; vector-effect: non-scaling-stroke; }
  .visual-candles .latest-line.falling { stroke: rgb(245 107 107 / 80%); }
  .visual-candles .selection-line { stroke: rgb(255 255 255 / 68%); stroke-width: 1; stroke-dasharray: 4 4; vector-effect: non-scaling-stroke; }
  .visual-candles text { fill: rgb(255 255 255 / 50%); font-size: 9px; font-family: var(--font-rounded); }
  .visual-candles .price-label { fill: rgb(255 255 255 / 68%); font-family: var(--font-mono-digits); }
  .close-chip { position: absolute; left: var(--space-4); z-index: 2; max-width: 118px; padding: var(--space-2) var(--space-7); border-radius: var(--pill-radius); background: var(--color-green); color: var(--color-on-accent); font-size: var(--type-candle-price-tag); font-weight: var(--weight-candle-price-tag); transform: translateY(-50%); white-space: nowrap; }
  .close-chip.falling { background: var(--color-red); }
  .empty { display: flex; height: 104px; align-items: center; justify-content: center; flex-direction: column; gap: var(--space-4); border-radius: var(--radius-8); background: rgb(255 255 255 / 2%); color: var(--color-text-secondary); }
  .empty span { font-size: var(--type-placeholder-icon); }.empty strong { font-size: var(--type-pill); font-weight: var(--weight-medium); }.empty small { color: var(--color-text-tertiary); font-size: var(--type-placeholder-hint); }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
  @media (prefers-contrast: more) { .card { background: var(--color-card-opaque); border-color: var(--color-card-stroke-strong); } }
  @media (prefers-reduced-transparency: reduce) { .card { background: var(--color-card-opaque); } }
</style>
