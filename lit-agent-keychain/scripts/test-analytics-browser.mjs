// Browser test with real production hostnames and intercepted ingestion. Sends
// no events to PostHog and makes no Lit, wallet, Stripe, or Google requests.
import { chromium } from "@playwright/test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "../..");
const browser = await chromium.launch();
try {
  const context = await browser.newContext();
  const events = [];
  const errors = [];
  await context.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.hostname === "us.i.posthog.com") {
      events.push(route.request().postDataJSON());
      return route.fulfill({
        status: 200,
        contentType: "application/json",
        body: '{"status":1}',
      });
    }
    if (url.hostname === "litprotocol.com") {
      if (process.env.MARKETING_TEST_ORIGIN) {
        const response = await route.fetch({
          url: process.env.MARKETING_TEST_ORIGIN + url.pathname + url.search,
        });
        return route.fulfill({ response });
      }
      if (url.pathname === "/analytics.js")
        return route.fulfill({
          contentType: "text/javascript",
          body: await readFile(
            path.join(root, "lit-static/analytics.js"),
            "utf8",
          ),
        });
      return route.fulfill({
        contentType: "text/html",
        body: '<script type="module">import {createAnalytics} from "/analytics.js"; window.analytics=createAnalytics("marketing"); window.analytics.page("/");</script>',
      });
    }
    const staticRoot =
      url.hostname === "dashboard.chipotle.litprotocol.com"
        ? path.join(root, "lit-static")
        : url.hostname === "keychain.litprotocol.com"
          ? path.join(root, "lit-agent-keychain/web/dist")
          : undefined;
    if (!staticRoot) return route.abort();
    if (url.pathname === "/api/config")
      return route.fulfill({
        contentType: "application/json",
        body: '{"network":"test","googleClientId":""}',
      });
    const file = path.resolve(
      staticRoot,
      "." + url.pathname + (url.pathname.endsWith("/") ? "index.html" : ""),
    );
    if (!file.startsWith(staticRoot + path.sep)) return route.abort();
    try {
      const contentType = file.endsWith(".js")
        ? "text/javascript"
        : file.endsWith(".css")
          ? "text/css"
          : file.endsWith(".html")
            ? "text/html"
            : "application/octet-stream";
      return route.fulfill({ contentType, body: await readFile(file) });
    } catch {
      return route.fulfill({ status: 404, body: "" });
    }
  });
  const page = await context.newPage();
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(
    "https://litprotocol.com/?utm_source=browser-test&token=private-token#secret-fragment",
  );
  if (process.env.MARKETING_TEST_ORIGIN) {
    await page.waitForTimeout(1000);
    assert.ok(
      events.some(
        (event) =>
          event.event === "$pageview" &&
          event.properties.product === "marketing",
      ),
    );
    // Exercise the real delegated CTA listener without leaving the current tab.
    await page.evaluate(() => {
      const a = document.createElement("a");
      a.href = "https://dashboard.chipotle.litprotocol.com/";
      a.addEventListener("click", (event) => event.preventDefault());
      document.body.append(a);
      a.click();
    });
    await page.waitForTimeout(100);
    assert.ok(events.some((event) => event.event === "product_cta_clicked"));
    assert.equal(await page.locator('script[src*="plausible"]').count(), 0);
  } else {
    await page.waitForFunction(() => !!window.analytics);
    await page.evaluate(() =>
      window.analytics.capture("product_cta_clicked", {
        destination: "dashboard",
      }),
    );
  }
  await page.goto(
    "https://dashboard.chipotle.litprotocol.com/dapps/dashboard/",
  );
  await page.locator("#login-api-key").waitFor({ state: "visible" });
  await page.locator("#login-api-key").fill("do-not-record-this-key");
  await page.waitForTimeout(100);
  await page.goto("https://keychain.litprotocol.com/");
  await page.getByRole("heading", { name: "Sign in to Keychain." }).waitFor();
  await page.waitForTimeout(100);
  const pages = events.filter((event) => event.event === "$pageview");
  assert.deepEqual(
    pages.map((event) => event.properties.product),
    ["marketing", "dashboard", "keychain"],
  );
  assert.equal(
    new Set(pages.map((event) => event.properties.distinct_id)).size,
    1,
  );
  assert.equal(
    new Set(pages.map((event) => event.properties.$session_id)).size,
    1,
  );
  const cookies = await context.cookies();
  assert.ok(
    cookies.some(
      (cookie) =>
        cookie.name === "lit_analytics_v1" &&
        cookie.domain === ".litprotocol.com" &&
        cookie.secure,
    ),
  );
  const payloads = JSON.stringify(events);
  for (const value of [
    "private-token",
    "secret-fragment",
    "do-not-record-this-key",
  ])
    assert.ok(!payloads.includes(value));
  assert.deepEqual(errors, []);
  console.log(
    "PASS: production-host browser journey, shared cookies/session, real dashboard + keychain render, no captured credentials or JS errors",
  );
} finally {
  await browser.close();
}
