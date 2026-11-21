import { chromium } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { join, resolve, extname } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
const fixture = JSON.parse(
  await readFile(
    process.argv[2] ?? new URL("./fixture.json", import.meta.url),
    "utf8",
  ),
);
const output = resolve(process.argv[3] || "test-results");
await mkdir(output, { recursive: true });
const dist = fileURLToPath(new URL("../dist", import.meta.url));
// Fulfill the desktop UI assets in-process: no network socket or dev server needed.
const url = "http://paddock.test";
const browser = await chromium.launch({ channel: "chrome", headless: true });
let checks = 0;
const errors = [];
const results = [];
async function context(
  width,
  height,
  scale = 1,
  data = fixture,
  filterCatalog = null,
  browserOptions = {},
) {
  const ctx = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: scale,
    colorScheme: width === 1360 ? "dark" : "light",
    ...browserOptions,
  });
  await ctx.route("**/*", async (route) => {
    const requestUrl = new URL(route.request().url());
    if (requestUrl.origin !== url) {
      errors.push("Unexpected network: " + requestUrl.href);
      await route.abort();
      return;
    }
    try {
      const target = resolve(
        dist,
        "." +
          (requestUrl.pathname === "/" ? "/index.html" : requestUrl.pathname),
      );
      if (!target.startsWith(dist + "\\")) throw Error("Invalid asset");
      const body = await readFile(target);
      await route.fulfill({
        status: 200,
        body,
        contentType:
          {
            ".html": "text/html",
            ".js": "text/javascript",
            ".css": "text/css",
            ".woff2": "font/woff2",
            ".svg": "image/svg+xml",
            ".png": "image/png",
          }[extname(target)] || "application/octet-stream",
      });
    } catch {
      await route.fulfill({ status: 404, body: "Not found" });
    }
  });
  await ctx.addInitScript(
    ({ data, filterCatalog }) => {
      const state = structuredClone(data);
      const catalog = filterCatalog ?? state.schedule?.series ?? [];
      if (state.schedule && !Array.isArray(state.schedule.lineup))
        state.schedule.lineup = catalog
          .filter((series) =>
            series.weeks.some(
              (week) =>
                week.number === state.schedule.current_week &&
                week.selected &&
                week.track,
            ),
          )
          .map((series) => {
            const week = series.weeks.find(
              (w) => w.number === state.schedule.current_week,
            );
            return {
              id: series.id,
              name: series.name,
              class: series.class,
              category: series.category,
              track: week.track,
              vehicles: week.vehicles || series.vehicles,
            };
          });
      window.__testCalls = [];
      window.ipc = {
        postMessage(message) {
          const request = JSON.parse(message);
          window.__testCalls.push(request);
          const a = request.args || {};
          if (request.command === "app.add_detected") {
            const found = state.detected_apps.find((app) => app.id === a.id);
            if (found && !found.added) {
              found.added = true;
              state.apps.push({
                id: found.id,
                index: state.apps.length,
                name: found.name,
                path: found.path,
                exists: true,
                running: 0,
                enabled: true,
                tray: false,
                hide_status: "",
              });
            }
          }
          if (request.command === "profile.delete") {
            state.profiles = state.profiles.filter(
              (profile) => profile.id !== a.id,
            );
            if (state.active_profile === a.id) {
              state.active_profile = state.profiles[0].id;
              state.profile_name = state.profiles[0].name;
              state.apps = [];
            }
          }
          if (request.command === "app.update")
            Object.assign(state.apps[a.index], a);
          if (request.command === "auto_hide") state.auto_hide = a.value;
          if (request.command === "planner.preferences") {
            state.preferences = a;
            if (filterCatalog)
              state.schedule.series = catalog.filter(
                (series) =>
                  (!a.query ||
                    series.name
                      .toLowerCase()
                      .includes(a.query.toLowerCase())) &&
                  (!a.classes.length || a.classes.includes(series.class)) &&
                  (!a.categories.length ||
                    a.categories.includes(series.category)) &&
                  (!a.category || a.category === series.category) &&
                  (!a.favorites_only || series.favorite),
              );
          }
          if (request.command === "planner.reset")
            Object.assign(state.preferences, {
              query: "",
              classes: [],
              categories: [],
              favorites_only: false,
              hidden_only: false,
            });
          if (request.command === "planner.select") {
            const row = state.schedule.series
              .find((series) => series.id === a.id)
              .weeks.find((week) => week.number === a.week);
            row.selected = !row.selected;
            if (a.week === state.schedule.current_week) {
              const series = catalog.find((s) => s.id === a.id);
              state.schedule.lineup = state.schedule.lineup.filter(
                (s) => s.id !== a.id,
              );
              if (row.selected)
                state.schedule.lineup.push({
                  id: series.id,
                  name: series.name,
                  class: series.class,
                  category: series.category,
                  track: row.track,
                  vehicles: row.vehicles || series.vehicles,
                });
            }
          }
          if (request.command === "planner.favorite") {
            const series = catalog.find((s) => s.id === a.id);
            series.favorite = !series.favorite;
            state.schedule.favorite_count = catalog.filter(
              (s) => s.favorite,
            ).length;
            state.schedule.series.sort(
              (a, b) => Number(b.favorite) - Number(a.favorite),
            );
          }
          if (request.command === "theme") state.theme = a.value;
          if (request.command === "profile.create") {
            const id = "new-game";
            state.profiles.push({ id, name: a.name, app_count: 0 });
            state.active_profile = id;
            state.profile_name = a.name;
            state.apps = [];
          }
          queueMicrotask(() =>
            window.__paddockReply({
              id: request.id,
              result: structuredClone(state),
              error: null,
            }),
          );
        },
      };
    },
    { data, filterCatalog },
  );
  return ctx;
}
async function healthy(page, label) {
  await page.getByRole("heading", { level: 1 }).waitFor();
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(100);
  assert.ok(
    await page.evaluate(() =>
      Array.from(document.fonts).some(
        (face) => face.family === "Figtree" && face.status === "loaded",
      ),
    ),
    "The self-hosted Figtree font must be loaded",
  );
  const spacingErrors = await page
    .locator("button.astryx-button")
    .evaluateAll((buttons) =>
      buttons.flatMap((button) => {
        const rect = button.getBoundingClientRect(),
          style = getComputedStyle(button);
        if (!rect.width || !rect.height) return [];
        const label =
          button.textContent.trim() || button.getAttribute("aria-label");
        const errors = [];
        if (
          Math.abs(
            parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
          ) > 0.5
        )
          errors.push(label + ": asymmetric padding");
        const expectedGap =
          parseFloat(getComputedStyle(document.documentElement).fontSize) / 2;
        if (Math.abs(parseFloat(style.gap) - expectedGap) > 0.5)
          errors.push(label + ": inconsistent icon gap");
        const content = button.firstElementChild;
        for (const child of content?.querySelectorAll("svg") ?? []) {
          const childRect = child.getBoundingClientRect();
          if (
            childRect.height > 0 &&
            (childRect.top < rect.top - 1 || childRect.bottom > rect.bottom + 1)
          )
            errors.push(label + ": content outside button");
        }
        return errors;
      }),
    );
  assert.deepEqual(spacingErrors, [], label + ": button spacing");
  const geometry = await page.evaluate(() => ({
    width: innerWidth,
    document: document.documentElement.scrollWidth,
    body: document.body.scrollWidth,
    main: document.querySelector("main")?.getBoundingClientRect().toJSON(),
    overflows: [...document.querySelectorAll('button,input,[role="switch"]')]
      .filter((element) => {
        const r = element.getBoundingClientRect();
        if (!r.width || !r.height) return false;
        let parent = element.parentElement;
        while (parent) {
          const style = getComputedStyle(parent);
          if (style.overflowX === "auto" || style.overflowX === "scroll")
            return false;
          parent = parent.parentElement;
        }
        return r.right > innerWidth + 2 || r.left < -2;
      })
      .map((e) => e.getAttribute("aria-label") || e.textContent)
      .slice(0, 10),
  }));
  assert.ok(
    geometry.document <= geometry.width + 1,
    `${label}: page overflow ${JSON.stringify(geometry)}`,
  );
  assert.ok(
    !geometry.overflows.length,
    `${label}: controls outside viewport ${JSON.stringify(geometry)}`,
  );
  checks++;
  results.push({ label, ...geometry });
}
try {
  for (const [width, height, scale] of [
    [360, 640, 1],
    [390, 844, 2],
    [640, 480, 1.25],
    [960, 720, 1.5],
    [1280, 720, 1],
    [1360, 960, 1],
    [1920, 1080, 2],
  ]) {
    const ctx = await context(width, height, scale);
    const page = await ctx.newPage();
    page.on("pageerror", (e) => errors.push(e.message));
    page.on("request", (request) => {
      if (!request.url().startsWith(url) && !request.url().startsWith("data:"))
        errors.push(`Unexpected network: ${request.url()}`);
    });
    for (const route of ["planner", "apps", "learn"]) {
      await page.goto(`${url}/#${route}`);
      await healthy(page, `${width}x${height}@${scale}:${route}`);
      assert.equal(
        await page
          .getByRole("combobox", { name: "Profile", exact: true })
          .count(),
        route === "apps" ? 1 : 0,
      );
      assert.equal(await page.title(), "Paddock · iRacing Companion");
      if ((width === 1360 || width === 390) && scale !== 1.5)
        await page.screenshot({
          path: join(output, `${route}-${width}.png`),
          fullPage: false,
        });
    }
    await ctx.close();
  }
  const ctx = await context(1360, 960);
  const page = await ctx.newPage();
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`${url}/#apps`);
  await page
    .getByRole("switch", { name: "Include iRacing UI", exact: true })
    .click();
  assert.equal(
    await page
      .getByRole("switch", { name: "Include iRacing UI", exact: true })
      .isChecked(),
    false,
  );
  checks++;
  const startup = page.getByRole("combobox", { name: "iRacing UI startup" });
  await startup.click();
  await page.getByRole("option", { name: "Background", exact: true }).click();
  assert.ok(
    (await page.evaluate(() => window.__testCalls)).some(
      (call) =>
        call.command === "app.update" &&
        call.args.index === 0 &&
        call.args.tray === true,
    ),
  );
  await startup.click();
  await page.getByRole("option", { name: "Window", exact: true }).click();
  assert.ok(
    (await page.evaluate(() => window.__testCalls)).some(
      (call) =>
        call.command === "app.update" &&
        call.args.index === 0 &&
        call.args.tray === false,
    ),
  );
  checks++;
  await page.getByRole("button", { name: "Options for iRacing UI", exact: true }).click();
  await page.getByRole("menuitem", { name: "Advanced", exact: true }).click();
  const policy=page.getByRole("switch", { name: "Hide management window after game exit", exact: true });
  assert.equal(await policy.isChecked(), false);
  await policy.click();
  assert.equal(await policy.isChecked(), true);
  assert.ok((await page.evaluate(()=>window.__testCalls)).some(call=>call.command==="app.update"&&call.args.rehide_after_game===true));
  await page.getByRole("button", {name:"Close",exact:true}).click();
  checks++;
  await page
    .getByRole("button", { name: "Manage profiles", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "New profile", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Name", exact: true })
    .fill("Assetto Corsa");
  await page
    .getByRole("button", { name: "Create profile", exact: true })
    .click();
  await page
    .getByRole("heading", { name: "Build this game's app list", exact: true })
    .waitFor();
  assert.ok(
    (await page.evaluate(() => window.__testCalls)).some(
      (call) =>
        call.command === "profile.create" && call.args.name === "Assetto Corsa",
    ),
  );
  checks++;
  await page.goto(`${url}/#planner`);
  const checkbox = page.getByRole("checkbox").first();
  const initial = await checkbox.isChecked();
  await checkbox.click();
  assert.equal(await checkbox.isChecked(), !initial);
  checks++;
  // The current week is informational; no manual week selector or Hidden feature remains.
  assert.equal(
    await page.getByRole("button", { name: "Today", exact: true }).count(),
    0,
  );
  assert.equal(await page.getByRole("button", { name: /^Hidden / }).count(), 0);
  assert.equal(
    await page.getByRole("button", { name: "01", exact: true }).count(),
    0,
  );
  const currentWeek = fixture.schedule.current_week;
  await page
    .getByText(
      "Current week " + currentWeek + " of " + fixture.schedule.weeks.length,
      { exact: true },
    )
    .waitFor();
  const weekColor = await page
    .getByRole("region", { name: "Season series", exact: true })
    .getByRole("table")
    .first()
    .getByText(String(currentWeek).padStart(2, "0"), { exact: true })
    .evaluate((el) => getComputedStyle(el).color);
  const otherWeek = fixture.schedule.weeks.find(
    (w) => w.number !== currentWeek,
  ).number;
  const otherColor = await page
    .getByRole("region", { name: "Season series", exact: true })
    .getByRole("table")
    .first()
    .getByText(String(otherWeek).padStart(2, "0"), { exact: true })
    .evaluate((el) => getComputedStyle(el).color);
  assert.notEqual(weekColor, otherColor, "Current week needs a visible accent");
  checks++;
  const nextWeek = fixture.schedule.weeks.find(
    (w) => w.number !== currentWeek,
  ).number;
  await page.evaluate(
    ({ fixture, nextWeek }) => {
      const next = structuredClone(fixture);
      next.schedule.current_week = nextWeek;
      window.dispatchEvent(new CustomEvent("paddock:state", { detail: next }));
    },
    { fixture, nextWeek },
  );
  await page
    .getByText(
      "Current week " + nextWeek + " of " + fixture.schedule.weeks.length,
      { exact: true },
    )
    .waitFor();
  await page.evaluate((fixture) => {
    const next = structuredClone(fixture);
    next.schedule.current_week = null;
    window.dispatchEvent(new CustomEvent("paddock:state", { detail: next }));
  }, fixture);
  await page.getByText("Season overview", { exact: true }).waitFor();
  checks++;
  await page.evaluate(
    (fixture) =>
      window.dispatchEvent(
        new CustomEvent("paddock:state", { detail: fixture }),
      ),
    fixture,
  );
  const hideButton = page.getByRole("button", {
    name: "Hide Paddock",
    exact: true,
  });
  assert.equal(await hideButton.getAttribute("data-variant"), "secondary");
  const hideStyle = await hideButton.evaluate((el) => ({
    border: getComputedStyle(el).borderTopWidth,
    borderStyle: getComputedStyle(el).borderTopStyle,
  }));
  assert.notEqual(hideStyle.border, "0px");
  assert.equal(hideStyle.borderStyle, "solid");
  checks++;
  await hideButton.click();
  assert.ok(
    (await page.evaluate(() => window.__testCalls)).some(
      (c) => c.command === "window.hide",
    ),
  );
  checks++;
  const search = page.getByRole("searchbox", {
    name: "Search series, cars or tracks",
  });
  await search.fill("Mazda");
  await page.waitForTimeout(400);
  assert.ok(
    (await page.evaluate(() => window.__testCalls)).some(
      (call) =>
        call.command === "planner.preferences" && call.args.query === "Mazda",
    ),
  );
  checks++;
  await page.evaluate(() => (document.documentElement.style.fontSize = "200%"));
  await healthy(page, "200% text scaling");
  await page
    .getByRole("button", {
      name: "Options for " + fixture.schedule.series[0].name,
      exact: true,
    })
    .click();
  await page
    .getByRole("menuitem", { name: "Series details", exact: true })
    .click();
  await page.getByRole("dialog").waitFor();
  const bounds = await page.getByRole("dialog").boundingBox();
  assert.ok(bounds.x >= 0 && bounds.x + bounds.width <= 1361);
  checks++;
  await page.getByRole("button", { name: "Close", exact: true }).last().click();
  await ctx.close();
  const largeFixture = structuredClone(fixture);
  largeFixture.schedule.lineup = [];
  largeFixture.schedule.series = Array.from({ length: 137 }, (_, index) => ({
    ...structuredClone(fixture.schedule.series[0]),
    id: "large-" + index,
    name: "Series " + (index + 1),
    favorite: false,
    weeks: fixture.schedule.series[0].weeks.map((week) => ({
      ...week,
      selected: false,
    })),
  }));
  const largeContext = await context(1360, 960, 1, largeFixture);
  const largePage = await largeContext.newPage();
  largePage.on("pageerror", (e) => errors.push(e.message));
  await largePage.goto(`${url}/#planner`);
  await largePage
    .getByRole("heading", { name: "Series 1", exact: true })
    .waitFor();
  assert.equal(await largePage.getByRole("table").count(), 12);
  assert.ok(
    (await largePage.evaluate(
      () => document.getElementsByTagName("*").length,
    )) < 5000,
  );
  checks++;
  assert.equal(
    await largePage
      .getByRole("navigation", { name: "Series pages", exact: true })
      .count(),
    0,
  );
  checks++;
  const catalogRegion = largePage.getByRole("region", {
    name: "Season series",
    exact: true,
  });
  const currentLabel =
    "Series 1 · Week " +
    fixture.schedule.current_week +
    " · " +
    fixture.schedule.series[0].weeks.find(
      (w) => w.number === fixture.schedule.current_week,
    ).track;
  const currentPlan = catalogRegion.getByRole("checkbox", {
    name: currentLabel,
    exact: true,
  });
  await currentPlan.check();
  const lineup = largePage.getByRole("region", {
    name: "This week's lineup",
    exact: true,
  });
  await lineup
    .getByRole("button", {
      name: "Remove Series 1 from this week's lineup",
      exact: true,
    })
    .waitFor();
  assert.equal(await currentPlan.isChecked(), true);
  checks++;
  const futureWeek = fixture.schedule.series[0].weeks.find(
    (w) => w.number !== fixture.schedule.current_week,
  );
  const future = catalogRegion.getByRole("checkbox", {
    name: "Series 1 · Week " + futureWeek.number + " · " + futureWeek.track,
    exact: true,
  });
  await future.check();
  assert.equal(
    await lineup.getByRole("button", { name: /^Remove / }).count(),
    1,
  );
  checks++;
  // Scroll the one continuous catalog; all 137 series must become reachable.
  while ((await catalogRegion.getByRole("table").count()) < 137) {
    const previous = await catalogRegion.getByRole("table").count();
    await largePage.locator("#planner-series-end").scrollIntoViewIfNeeded();
    await largePage.waitForFunction(
      (previous) =>
        document
          .querySelector('section[aria-label="Season series"]')
          .querySelectorAll("table").length > previous,
      previous,
    );
  }
  assert.equal(await catalogRegion.getByRole("table").count(), 137);
  assert.equal(await currentPlan.isChecked(), true);
  checks++;
  await catalogRegion
    .getByRole("button", { name: "Favorite Series 137", exact: true })
    .click();
  await largePage.waitForFunction(
    () =>
      document.querySelector(
        'section[aria-label="Season series"] :is(h1,h2,h3,h4,[role="heading"])',
      )?.textContent === "Series 137",
  );
  assert.equal(
    await catalogRegion.getByRole("heading").first().textContent(),
    "Series 137",
  );
  checks++;
  await lineup
    .getByRole("button", {
      name: "Remove Series 1 from this week's lineup",
      exact: true,
    })
    .click();
  assert.equal(await currentPlan.isChecked(), false);
  assert.equal(await future.isChecked(), true);
  assert.equal(await lineup.getByRole("table").count(), 0);
  checks++;
  await largePage
    .getByRole("searchbox", { name: "Search series, cars or tracks" })
    .fill("Mazda");
  await largePage.waitForFunction(
    () =>
      document
        .querySelector('section[aria-label="Season series"]')
        .querySelectorAll("table").length === 12,
  );
  checks++;
  // A filtered-out planned series stays visible in the independent lineup summary.
  await largePage.evaluate((fixture) => {
    const next = structuredClone(fixture),
      series = next.schedule.series[0],
      week = series.weeks.find((w) => w.number === next.schedule.current_week);
    next.schedule.series = [];
    next.schedule.lineup = [
      {
        id: series.id,
        name: series.name,
        class: series.class,
        category: series.category,
        track: week.track,
        vehicles: week.vehicles || series.vehicles,
      },
    ];
    window.dispatchEvent(new CustomEvent("paddock:state", { detail: next }));
  }, fixture);
  await largePage
    .getByRole("heading", { name: "No matching series", exact: true })
    .waitFor();
  assert.equal(
    await lineup.getByRole("button", { name: /^Remove / }).count(),
    1,
  );
  checks++;
  await largePage.screenshot({
    path: join(output, "lineup-filtered-1360.png"),
    fullPage: false,
  });
  // The disclosure saves presentation state without changing any plans.
  const disclosure = lineup.getByRole("button", {
    name: /^This week's lineup/,
  });
  assert.equal(await disclosure.getAttribute("aria-expanded"), "true");
  const planCallsBefore = await largePage.evaluate(
    () =>
      window.__testCalls.filter((call) => call.command === "planner.select")
        .length,
  );
  await disclosure.click();
  assert.equal(await disclosure.getAttribute("aria-expanded"), "false");
  assert.equal(
    await lineup.getByRole("table", { name: "Weekly lineup" }).count(),
    0,
  );
  assert.ok((await disclosure.innerText()).includes("1 series"));
  assert.equal(
    await largePage.evaluate(
      () =>
        window.__testCalls.filter((call) => call.command === "planner.select")
          .length,
    ),
    planCallsBefore,
  );
  checks++;
  await largePage.screenshot({
    path: join(output, "lineup-collapsed-1360.png"),
  });
  await largePage.getByRole("link", { name: "Apps", exact: true }).click();
  await largePage.getByRole("link", { name: "Planner", exact: true }).click();
  assert.equal(await disclosure.getAttribute("aria-expanded"), "false");
  checks++;
  await largePage.reload();
  await disclosure.waitFor();
  assert.equal(await disclosure.getAttribute("aria-expanded"), "false");
  await disclosure.focus();
  await largePage.keyboard.press("Enter");
  assert.equal(await disclosure.getAttribute("aria-expanded"), "true");
  assert.equal(
    await lineup.getByText(/Check Plan on the highlighted/).isVisible(),
    true,
  );
  checks++;
  await largeContext.close();
  // Jump to a series outside the initial batch, preserving the current view.
  const jumpFixture = structuredClone(largeFixture);
  const destination = jumpFixture.schedule.series[136];
  const destinationWeek = destination.weeks.find(
    (week) => week.number === jumpFixture.schedule.current_week,
  );
  jumpFixture.schedule.lineup = [
    {
      id: destination.id,
      name: destination.name,
      class: destination.class,
      category: destination.category,
      track: destinationWeek.track,
      vehicles: destination.vehicles,
    },
  ];
  for (const filtered of [false, true]) {
    const data = structuredClone(jumpFixture);
    if (filtered) {
      data.schedule.series = [];
      Object.assign(data.preferences, {
        query: "No matching query",
        classes: ["Pro"],
        categories: ["Oval"],
        category: "Oval",
        favorites_only: true,
        sort: "Name",
      });
    }
    const jumpContext = await context(
      1360,
      960,
      1,
      data,
      filtered ? jumpFixture.schedule.series : null,
    );
    const jumpPage = await jumpContext.newPage();
    jumpPage.on("pageerror", (error) => errors.push(error.message));
    await jumpPage.goto(url + "/#planner");
    const jumpLineup = jumpPage.getByRole("region", {
      name: "This week's lineup",
      exact: true,
    });
    const nameLink = jumpLineup.getByRole("link", {
      name: destination.name,
      exact: true,
    });
    const headerGap = await jumpLineup.evaluate((section) => {
      const trigger = section.querySelector("button[aria-expanded]");
      const header = section.querySelector("thead");
      return (
        header.getBoundingClientRect().top -
        trigger.getBoundingClientRect().bottom
      );
    });
    assert.ok(
      headerGap >= 16,
      "Lineup header needs visible separation: " + headerGap,
    );
    const targetId = "planner-series-" + encodeURIComponent(destination.id);
    assert.equal(await jumpPage.locator('[id="' + targetId + '"]').count(), 0);
    await nameLink.click();
    await jumpPage.waitForFunction(
      (id) => document.activeElement?.id === id,
      targetId,
    );
    await jumpPage.waitForFunction((id) => {
      const r = document.getElementById(id)?.getBoundingClientRect();
      return r && r.top >= -1 && r.top < innerHeight;
    }, targetId);
    assert.equal(
      await jumpPage
        .getByRole("region", { name: "Season series", exact: true })
        .getByRole("table")
        .count(),
      137,
    );
    const calls = await jumpPage.evaluate(() => window.__testCalls);
    const changes = calls.filter(
      (call) => call.command === "planner.preferences",
    );
    assert.equal(changes.length, filtered ? 1 : 0);
    if (filtered) {
      assert.deepEqual(changes[0].args, {
        ...data.preferences,
        query: "",
        classes: [],
        categories: [],
        category: null,
        favorites_only: false,
      });
      assert.equal(await jumpPage.getByRole("searchbox").inputValue(), "");
    }
    assert.equal(
      calls.some((call) => call.command === "planner.select"),
      false,
    );
    checks++;
    await jumpPage.screenshot({
      path: join(
        output,
        "series-jump-" + (filtered ? "filtered" : "batch") + "-1360.png",
      ),
    });
    // Repeating the jump by keyboard works without changing the page route.
    await nameLink.focus();
    await jumpPage.keyboard.press("Enter");
    await jumpPage.waitForFunction(
      (id) => document.activeElement?.id === id,
      targetId,
    );
    assert.equal(new URL(jumpPage.url()).hash, "#planner");
    checks++;
    const back = jumpPage.getByRole("button", {
      name: "Back to top",
      exact: true,
    });
    await back.waitFor();
    const circle = await back.boundingBox();
    assert.ok(Math.abs(circle.width - circle.height) < 1);
    assert.ok(
      circle.x > 1200 && circle.y > 850,
      "Back to top belongs at lower right",
    );
    await jumpPage.screenshot({ path: join(output, "back-to-top-1360.png") });
    if (filtered) {
      await back.focus();
      await jumpPage.keyboard.press("Enter");
    } else await back.click();
    await back.waitFor({ state: "hidden" });
    await jumpPage.waitForFunction(() => {
      const h = document.querySelector("h1");
      const r = h.getBoundingClientRect();
      return r.top >= 0 && r.bottom < innerHeight;
    });
    assert.equal(
      await jumpPage
        .getByRole("heading", { level: 1 })
        .evaluate((h) => h === document.activeElement),
      true,
    );
    checks++;
    await jumpContext.close();
  }
  const mobileContext = await context(390, 844);
  const mobilePage = await mobileContext.newPage();
  await mobilePage.emulateMedia({ reducedMotion: "reduce" });
  await mobilePage.goto(url + "/#planner");
  await mobilePage.getByRole("heading", { level: 1 }).waitFor();
  const mobileBack = mobilePage.getByRole("button", {
    name: "Back to top",
    exact: true,
  });
  assert.equal(await mobileBack.count(), 0);
  await mobilePage
    .getByRole("region", { name: "Season series", exact: true })
    .getByRole("heading")
    .last()
    .scrollIntoViewIfNeeded();
  await mobileBack.waitFor();
  const mobileBox = await mobileBack.boundingBox();
  assert.ok(
    mobileBox.x > 300 &&
      mobileBox.x + mobileBox.width < 390 &&
      mobileBox.y > 740 &&
      mobileBox.y + mobileBox.height < 844,
  );
  await mobilePage.screenshot({ path: join(output, "back-to-top-390.png") });
  await mobileBack.click();
  await mobileBack.waitFor({ state: "hidden" });
  assert.equal(
    await mobilePage
      .getByRole("heading", { level: 1 })
      .evaluate((h) => h === document.activeElement),
    true,
  );
  checks++;
  await mobileContext.close();
  for (const width of [1360, 390]) {
    const data = structuredClone(fixture);
    data.profiles = [
      { id: "first", name: "First game", app_count: 0 },
      { id: "second", name: "Second game", app_count: 0 },
    ];
    data.active_profile = "second";
    data.profile_name = "Second game";
    data.detected_apps = [
      {
        id: "detected-crew",
        name: "Crew Chief",
        path: "D:/Program Files/Britton IT Ltd/CrewChiefV4/CrewChiefV4.exe",
        added: false,
      },
      {
        id: "simhub",
        name: "SimHub",
        path: "D:/Program Files/SimHub/SimHubWPF.exe",
        added: true,
      },
    ];
    const appContext = await context(width, 960, 1, data),
      appPage = await appContext.newPage();
    await appPage.goto(url + "/#apps");
    await healthy(appPage, "discovery-" + width);
    const detected = appPage.getByRole("region", {
      name: "Detected apps",
      exact: true,
    });
    await detected
      .getByRole("button", { name: "Add Crew Chief", exact: true })
      .click();
    assert.equal(await detected.getByText("Added", { exact: true }).count(), 2);
    assert.equal(
      await detected
        .getByRole("button", { name: "Add Crew Chief", exact: true })
        .count(),
      0,
    );
    await detected
      .getByRole("button", { name: "Scan again", exact: true })
      .click();
    await appPage
      .getByRole("button", { name: "Browse executable", exact: true })
      .click();
    const commands = await appPage.evaluate(() =>
      window.__testCalls.map((call) => call.command),
    );
    assert.ok(
      commands.includes("app.scan") &&
        commands.includes("app.add_detected") &&
        commands.includes("app.add"),
    );
    checks++;
    await detected.scrollIntoViewIfNeeded();
    await appPage.screenshot({
      path: join(output, "detected-apps-" + width + ".png"),
    });
    if (width === 1360) {
      await appPage
        .getByRole("combobox", { name: "Profile", exact: true })
        .click({ button: "right" });
      await appPage
        .getByRole("menuitem", { name: "Delete profile", exact: true })
        .click();
      await appPage
        .getByRole("dialog")
        .getByRole("button", { name: "Cancel", exact: true })
        .click();
      assert.equal(
        (await appPage.evaluate(() => window.__testCalls)).some(
          (call) => call.command === "profile.delete",
        ),
        false,
      );
      await appPage
        .getByRole("combobox", { name: "Profile", exact: true })
        .click({ button: "right" });
      await appPage
        .getByRole("menuitem", { name: "Delete profile", exact: true })
        .click();
      await appPage
        .getByRole("dialog")
        .getByRole("button", { name: "Delete profile", exact: true })
        .click();
      assert.equal(
        await appPage
          .getByRole("combobox", { name: "Profile", exact: true })
          .innerText(),
        "First game",
      );
      await appPage
        .getByRole("button", { name: "Manage profiles", exact: true })
        .click();
      assert.equal(
        await appPage
          .getByRole("menuitem", { name: "Delete profile", exact: true })
          .getAttribute("aria-disabled"),
        "true",
      );
      checks++;
    }
    await appContext.close();
  }
  for (const [timezoneId, day, hour] of [
    ["America/Los_Angeles", "Nov 16, 2026", "04:00 PM"],
    ["Asia/Seoul", "Nov 17, 2026", "09:00 AM"],
  ]) {
    const localContext = await context(1360, 960, 1, fixture, null, {
        timezoneId,
        locale: "en-US",
      }),
      localPage = await localContext.newPage();
    await localPage.goto(url + "/#planner");
    const timing = localPage.getByText(/^Starts /);
    await timing.waitFor();
    const value = await timing.innerText();
    assert.ok(
      value.includes(day) &&
        value.includes(hour) &&
        value.includes("Next reset"),
      value,
    );
    assert.ok(!value.includes("Resets Tuesday, 09:00 KST"));
    checks++;
    await localPage.screenshot({
      path: join(
        output,
        "local-time-" + timezoneId.replaceAll("/", "-") + ".png",
      ),
    });
    await localContext.close();
  }
  assert.deepEqual(errors, []);
  checks++;
  await writeFile(
    join(output, "results.json"),
    JSON.stringify({ passed: checks, errors, results }, null, 2),
  );
  console.log(
    `PASS ${checks} responsive, scaling, interaction and offline checks`,
  );
} finally {
  await browser.close();
}
