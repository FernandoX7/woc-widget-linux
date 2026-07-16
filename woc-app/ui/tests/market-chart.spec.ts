import { expect, test } from '@playwright/test';

test('chart formatting stays fixed point at four significant figures', async ({ page }) => {
  await page.goto('/?state=live&page=market');
  const values = await page.evaluate(async () => {
    const chart = await import('/src/lib/market/chart.ts');
    return {
      current: chart.chartPrice(0.0005594),
      tiny: chart.chartPrice(0.00005594),
      capped: chart.chartPrice(1e-13),
      extreme: chart.chartPrice(1e-100),
      aboveOne: chart.chartPrice(1.5),
      zero: chart.chartPrice(0),
      flat: chart.candleDomain([{ time: 1, open: 10, high: 10, low: 10, close: 10, volume: 1 }]),
      normal: chart.candleDomain([
        { time: 1, open: 8, high: 10, low: 5, close: 9, volume: 1 },
        { time: 2, open: 9, high: 12, low: 7, close: 11, volume: 1 },
      ]),
      clamped: chart.candleDomain([{ time: 1, open: 1, high: 10, low: 0.1, close: 2, volume: 1 }]),
    };
  });
  expect(values.current).toBe('0.0005594');
  expect(values.tiny).toBe('0.00005594');
  expect(values.capped).toBe('0.000000000000');
  expect(values.extreme).toBe('0.000000000000');
  expect(values.aboveOne).toBe('1.500');
  expect(values.zero).toBe('0');
  expect(values.flat).toEqual({ min: 9.984, max: 10.016 });
  expect(values.normal?.min).toBeCloseTo(4.44);
  expect(values.normal?.max).toBeCloseTo(12.56);
  expect(values.clamped?.min).toBe(0);
  expect(values.clamped?.max).toBeCloseTo(10.792);
  expect(Object.values(values).join('')).not.toMatch(/[eE][+-]?\d/);
});

test('candlestick card renders native chart, latest OHLC, and keyboard inspection', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/?state=live&page=market');

  await expect(page.getByRole('heading', { name: '$WOC PRICE', exact: true })).toBeVisible();
  await expect(page.locator('.detail')).toHaveAttribute('aria-label', /open .* high .* low .* close/);
  await expect(page.locator('.close-chip')).toHaveText(/^5m close \$0\.\d+$/);
  await expect(page.locator('#market-candle-data li')).toHaveCount(60);
  await expect(page.locator('.visual-candles .candle')).toHaveCount(60);
  await expect(page.locator('.visual-candles .wick')).toHaveCount(60);
  await expect(page.locator('.visual-candles .body')).toHaveCount(60);
  await expect(page.locator('.visual-candles .rising').first()).toBeVisible();
  await expect(page.locator('.visual-candles .falling').first()).toBeVisible();
  await expect(page.locator('.visual-candles .rising .body').first()).toHaveCSS('fill', 'rgb(84, 219, 125)');
  await expect(page.locator('.visual-candles .falling .body').first()).toHaveCSS('fill', 'rgba(0, 0, 0, 0)');
  const plot = page.getByRole('button', { name: /\$WOC candlestick chart/ });
  await plot.focus();
  await page.keyboard.press('ArrowRight');
  await expect(page.getByText('⌖ SELECTED', { exact: true })).toBeVisible();
  await expect(page.locator('.visual-candles .selection-line')).toHaveCount(1);
  await expect(page.locator('.visual-candles .selection-line')).toHaveCSS('stroke', 'rgba(255, 255, 255, 0.68)');
  await page.keyboard.press('Escape');
  await expect(page.getByText('◷ LATEST', { exact: true })).toBeVisible();
  await expect(page.locator('.visual-candles .selection-line')).toHaveCount(0);

  const latestDetail = await page.locator('.detail').getAttribute('aria-label');
  const box = await plot.boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.move(box!.x + box!.width * 0.2, box!.y + box!.height * 0.5);
  await expect(page.getByText('⌖ SELECTED', { exact: true })).toBeVisible();
  await expect.poll(() => page.locator('.detail').getAttribute('aria-label')).not.toBe(latestDetail);

  await page.screenshot({ path: 'test-results/phase-11-candlestick-hollow.png' });
  expect(errors).toEqual([]);
});

test('interval change discards mismatched candles until the requested interval loads', async ({ page }) => {
  await page.goto('/?state=live&page=market');
  await expect(page.getByRole('button', { name: /\$WOC candlestick chart/ })).toBeVisible();
  await page.getByRole('button', { name: '1m', exact: true }).click();
  await expect(page.getByRole('button', { name: /\$WOC candlestick chart/ })).toBeHidden();
  await expect(page.getByText('Chart unavailable', { exact: true })).toBeVisible();
  await expect(page.getByText('Showing cached 5m candles while the new interval loads')).toBeVisible();
  await expect(page.getByRole('button', { name: '1m', exact: true })).toHaveAttribute('aria-pressed', 'true');
});
