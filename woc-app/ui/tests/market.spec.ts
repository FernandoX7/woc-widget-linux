import { expect, test } from '@playwright/test';

for (const scenario of ['live','quote-only','chart-only','cached-offline'] as const) {
  test(`${scenario} market fixture preserves independent feeds`, async ({ page }) => {
    const errors:string[]=[];
    page.on('console', message=>{ if(message.type()==='error') errors.push(message.text()); });
    page.on('pageerror', error=>errors.push(error.message));
    await page.goto(`/?state=${scenario}&page=market`);
    await expect(page.getByRole('heading',{name:'$WOC SPOT'})).toBeVisible();
    await expect(page.locator('.feed-pill').filter({hasText:'Quote'})).toBeVisible();
    await expect(page.locator('.feed-pill').filter({hasText:'Chart'})).toBeVisible();
    if (scenario === 'quote-only') {
      await expect(page.locator('.spot .price-row strong')).toHaveText('$0.0005594');
      await expect(page.locator('.feed-pill').filter({hasText:'Chart unavailable'})).toBeVisible();
      await expect(page.getByLabel('$WOC PRICE').getByText('Chart unavailable')).toBeVisible();
      await expect(page.getByText('Fully diluted value', {exact:true})).toBeVisible();
      await expect(page.getByText('$999K', {exact:true})).toBeVisible();
    } else if (scenario === 'chart-only') {
      await expect(page.getByText('Quote unavailable')).toBeVisible();
      await expect(page.getByRole('button',{name:/\$WOC candlestick chart/})).toBeVisible();
    } else if (scenario === 'cached-offline') {
      await expect(page.getByText('Quote cached')).toBeVisible();
      await expect(page.getByText('Chart cached')).toBeVisible();
      await expect(page.getByText('updated 2h ago')).toBeVisible();
    } else {
      await expect(page.getByText('Quote live')).toBeVisible();
      await expect(page.getByText('Chart live')).toBeVisible();
      await expect(page.getByText('$81.08K')).toBeVisible();
      await expect(page.getByText('526 / 508')).toBeVisible();
      await expect(page.getByText('Buys 51%')).toBeVisible();
    }
    await page.screenshot({ path:`test-results/phase-11-${scenario}.png` });
    expect(errors).toEqual([]);
  });
}

test('spot timeframe updates change, metrics, and activity', async ({page})=>{
  await page.goto('/?state=live&page=market');
  const windows = [
    ['5m', '+1.2%', '$912', '8 / 5'],
    ['1h', '+6.8%', '$9.42K', '72 / 48'],
    ['6h', '+18.4%', '$48.2K', '321 / 264'],
    ['24h', '+47.6%', '$81.08K', '526 / 508'],
  ] as const;
  for (const [timeframe, change, volume, transactions] of windows) {
    await page.getByRole('button',{name:timeframe,exact:true}).first().click();
    await expect(page.locator('.spot .price-row')).toContainText(change);
    await expect(page.getByText(volume,{exact:true})).toBeVisible();
    await expect(page.getByText(transactions,{exact:true})).toBeVisible();
    await expect(page.locator('.activity')).toHaveAttribute('aria-label', new RegExp(`^${timeframe} activity:`));
  }
});

test('copy contract writes the exact address and shows two-second feedback', async ({page,context})=>{
  await context.grantPermissions(['clipboard-read','clipboard-write']);
  await page.goto('/?state=live&page=market');
  await page.getByRole('button',{name:'Copy contract'}).click();
  await expect(page.getByRole('button',{name:'Contract copied'})).toBeVisible();
  expect(await page.evaluate(()=>navigator.clipboard.readText())).toBe('3WjLscH2JsXLEFJZRA9z8ti8yRGxWGKbqymPd7UicRth');
  await expect(page.getByRole('button',{name:'Copy contract'})).toBeVisible({timeout:2_500});
});

test('View rejects a hostile fixture pair URL and opens the canonical market URL', async ({page})=>{
  await page.goto('/?state=quote-only&page=market');
  await page.locator('.actions .view').click();
  await expect.poll(() => page.locator('html').getAttribute('data-fixture-opened-url'))
    .toBe('https://dexscreener.com/solana/5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p');
});

test('View preserves a valid fixture pair URL', async ({page})=>{
  await page.goto('/?state=live&page=market');
  await page.locator('.actions .view').click();
  await expect.poll(() => page.locator('html').getAttribute('data-fixture-opened-url'))
    .toBe('https://dexscreener.com/solana/5we9yjzpeqxcyl4jn9khjtsr48xzyh47xtar9kg3wy1p');
});
