// FV1 screenshots (1440x900): the first visit, and a new request just added.
//   HITOP_MODE=tauri node tests/shots-fv1.cjs <dir>
const path = require('path');
const { launch } = require('./harness.cjs');

(async () => {
  const dir = process.argv[2] || '.';
  const h = await launch({ viewport: { width: 1440, height: 900 } });
  const p = h.page;
  await p.goto(h.url, { waitUntil: 'networkidle' });
  await p.waitForTimeout(800);
  await p.screenshot({ path: path.join(dir, `first-visit-${h.mode}.png`) });
  await p.locator('.tab-strip').getByRole('button', { name: 'New request' }).click();
  await p.waitForTimeout(500);
  await p.keyboard.type('https://api.example.com/users');
  await p.waitForTimeout(500);
  await p.screenshot({ path: path.join(dir, `new-request-${h.mode}.png`) });
  await h.finish();
})();
