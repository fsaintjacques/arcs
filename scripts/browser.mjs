/**
 * One place that knows where Chromium is.
 *
 * The sandbox this repo is usually driven from ships a browser at
 * `/opt/pw-browsers/chromium`; a plain checkout has whatever
 * `npx playwright install chromium` put in Playwright's own cache. Every
 * screenshot and check script asks here rather than hard-coding one of them.
 */
import { existsSync } from 'node:fs';
import { chromium } from 'playwright';

const SANDBOX_CHROMIUM = '/opt/pw-browsers/chromium';

/** Where `npm run dev` is serving; override with `ARCS_URL` when Vite picks another port. */
export const appUrl = process.env.ARCS_URL ?? 'http://localhost:5173/';

export function launchChromium(options = {}) {
  const executablePath = process.env.PW_CHROMIUM ?? SANDBOX_CHROMIUM;
  return chromium.launch({
    ...(existsSync(executablePath) ? { executablePath } : {}),
    ...options,
  });
}
