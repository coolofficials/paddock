import { chromium } from "@playwright/test";
import { createServer } from "node:http";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, extname, sep } from "node:path";
import { performance } from "node:perf_hooks";
const fixture = JSON.parse(await readFile(process.argv[2], "utf8"));
const dist = resolve("dist");
const server = createServer(async (req, res) => {
  try {
    const name = new URL(req.url, "http://localhost").pathname;
    const target = resolve(dist, "." + (name === "/" ? "/index.html" : name));
    if (!target.startsWith(dist + sep)) throw Error("Invalid path");
    res.setHeader(
      "content-type",
      { ".html": "text/html", ".js": "text/javascript", ".css": "text/css" }[
        extname(target)
      ] || "application/octet-stream",
    );
    res.end(await readFile(target));
  } catch {
    res.writeHead(404);
    res.end();
  }
});
await new Promise((done) => server.listen(0, "127.0.0.1", done));
const browser = await chromium.launch({ channel: "chrome", headless: true });
const context = await browser.newContext({
  viewport: { width: 1360, height: 960 },
});
await context.addInitScript((data) => {
  window.ipc = {
    postMessage(message) {
      const request = JSON.parse(message);
      queueMicrotask(() =>
        window.__paddockReply({
          id: request.id,
          result: structuredClone(data),
          error: null,
        }),
      );
    },
  };
}, fixture);
const samples = [];
try {
  for (let i = 0; i < 3; i++) {
    const page = await context.newPage();
    let start = performance.now();
    await page.goto(`http://127.0.0.1:${server.address().port}/#planner`);
    await page
      .getByRole("heading", { name: "Season planner", exact: true })
      .waitFor();
    await page.evaluate(() => document.fonts.ready);
    await page.evaluate(
      () =>
        new Promise((done) =>
          requestAnimationFrame(() => requestAnimationFrame(done)),
        ),
    );
    const loadMs = performance.now() - start;
    const dom = await page.evaluate(() => ({
      elements: document.getElementsByTagName("*").length,
      checkboxes: document.querySelectorAll('input[type="checkbox"]').length,
    }));
    start = performance.now();
    await page.getByRole("link", { name: "Apps", exact: true }).click();
    await page
      .getByRole("heading", { name: "App manager", exact: true })
      .waitFor();
    await page.evaluate(
      () =>
        new Promise((done) =>
          requestAnimationFrame(() => requestAnimationFrame(done)),
        ),
    );
    const toAppsMs = performance.now() - start;
    start = performance.now();
    await page.getByRole("link", { name: "Planner", exact: true }).click();
    await page
      .getByRole("heading", { name: "Season planner", exact: true })
      .waitFor();
    await page.evaluate(
      () =>
        new Promise((done) =>
          requestAnimationFrame(() => requestAnimationFrame(done)),
        ),
    );
    samples.push({
      loadMs,
      toAppsMs,
      toPlannerMs: performance.now() - start,
      ...dom,
    });
    await page.close();
  }
  const mean = (key) =>
    samples.reduce((sum, s) => sum + s[key], 0) / samples.length;
  const result = {
    series: fixture.schedule.series.length,
    viewport: "1360x960",
    samples,
    mean: Object.fromEntries(
      ["loadMs", "toAppsMs", "toPlannerMs", "elements", "checkboxes"].map(
        (key) => [key, Math.round(mean(key))],
      ),
    ),
  };
  await writeFile(process.argv[3], JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result.mean));
} finally {
  await browser.close();
  await new Promise((done) => server.close(done));
}
