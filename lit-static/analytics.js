// Shared browser analytics transport. Synced to Keychain, the docs site, and the
// marketing repo by scripts/sync-product-analytics.mjs. No remote executable
// code or DOM capture.
const TOKEN = "phc_CC3jC8wKzwQwgPjkt8jajRMXnoGutewKf8zxGQohCX2C";
const HOST = "https://us.i.posthog.com";
const COOKIE = "lit_analytics_v1";
const PRODUCTION = {
  marketing: ["litprotocol.com", "www.litprotocol.com"],
  dashboard: ["dashboard.chipotle.litprotocol.com"],
  keychain: ["keychain.litprotocol.com"],
  docs: ["developer.litprotocol.com"],
};
const FIELDS = new Set([
  "section",
  "method",
  "operation",
  "outcome",
  "plan",
  "destination",
  "step",
  "actor_type",
  "amount_cents",
  "currency",
  "mode",
]);
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** Explicit events only. Analytics must never interrupt a product operation. */
export function createAnalytics(product, browser = globalThis) {
  let memory;
  let person;
  let account;
  let lastPage;
  const enabled = () =>
    PRODUCTION[product]?.includes(browser.location?.hostname) &&
    browser.location?.protocol === "https:";
  const fresh = () => ({
    anonymous: browser.crypto.randomUUID(),
    session: browser.crypto.randomUUID(),
    at: Date.now(),
  });
  const write = (state) => {
    memory = state;
    try {
      browser.document.cookie = `${COOKIE}=${encodeURIComponent(JSON.stringify(state))}; Domain=litprotocol.com; Path=/; Max-Age=31536000; SameSite=Lax; Secure`;
    } catch {
      /* In-memory fallback when cookies are unavailable. */
    }
  };
  const read = () => {
    let state = memory;
    try {
      const value = browser.document.cookie
        .split("; ")
        .find((v) => v.startsWith(`${COOKIE}=`));
      if (value) {
        const candidate = JSON.parse(
          decodeURIComponent(value.slice(COOKIE.length + 1)),
        );
        if (
          UUID.test(candidate.anonymous) &&
          UUID.test(candidate.session) &&
          Number.isFinite(candidate.at) &&
          (!candidate.person ||
            /^(dashboard-user:|wallet:|keychain-owner:)[a-zA-Z0-9_-]{1,128}$/.test(
              candidate.person,
            ))
        )
          state = candidate;
      }
    } catch {
      /* Malformed/blocked cookies must not break the application. */
    }
    state ||= fresh();
    if (Date.now() - state.at > 30 * 60 * 1000 || state.at > Date.now())
      state.session = browser.crypto.randomUUID();
    state.at = Date.now();
    write(state);
    return state;
  };
  const attribution = () => {
    const props = {};
    try {
      const params = new URL(browser.location.href).searchParams;
      for (const key of [
        "utm_source",
        "utm_medium",
        "utm_campaign",
        "utm_content",
        "utm_term",
      ]) {
        const value = params.get(key);
        if (value && /^[a-zA-Z0-9 _./:-]{1,100}$/.test(value))
          props[key] = value;
      }
      const ref = new URL(browser.document.referrer);
      props.$referring_domain = ref.hostname;
    } catch {
      /* Empty referrer. */
    }
    return props;
  };
  const send = (event, properties = {}) => {
    if (!enabled()) return;
    try {
      const state = read();
      const origin = browser.location.origin;
      // Marketing and docs paths are public page names; app paths may carry state.
      const path =
        product === "marketing" || product === "docs"
          ? browser.location.pathname
          : product === "dashboard"
            ? "/dapps/dashboard/"
            : "/";
      const acquisition = attribution();
      const initial = Object.fromEntries(
        Object.entries(acquisition).map(([k, v]) => [
          k.startsWith("$") ? `$initial_${k.slice(1)}` : `$initial_${k}`,
          v,
        ]),
      );
      const payload = {
        api_key: TOKEN,
        distinct_id: person || state.person || state.anonymous,
        uuid: browser.crypto.randomUUID(),
        event,
        timestamp: new Date().toISOString(),
        properties: {
          distinct_id: person || state.person || state.anonymous,
          $device_id: state.anonymous,
          $session_id: state.session,
          $current_url: origin + path,
          $host: browser.location.hostname,
          $pathname: path,
          $lib: "lit-browser",
          $lib_version: "1",
          $process_person_profile: true,
          $is_identified: !!(person || state.person),
          $set_once: initial,
          product,
          environment: "production",
          source: "browser",
          schema_version: 1,
          ...acquisition,
          ...(account
            ? {
                $groups: { billing_account: account },
                billing_account: account,
              }
            : {}),
          ...properties,
        },
      };
      // text/plain avoids a preflight. No application cookies or Referer header
      // (auth links can contain secrets). keepalive survives checkout navigation.
      void browser
        .fetch(`${HOST}/i/v0/e/`, {
          method: "POST",
          headers: { "Content-Type": "text/plain" },
          body: JSON.stringify(payload),
          credentials: "omit",
          referrerPolicy: "no-referrer",
          keepalive: true,
        })
        .catch(() => {});
    } catch {
      /* Best-effort telemetry; never fail a user operation. */
    }
  };
  const api = {
    capture(event, properties = {}) {
      if (!/^[a-z][a-z0-9_]{0,63}$/.test(event)) return;
      const safe = {};
      for (const [key, value] of Object.entries(properties)) {
        if (
          FIELDS.has(key) &&
          ((typeof value === "string" &&
            /^[a-zA-Z0-9 _.:/-]{1,100}$/.test(value)) ||
            (typeof value === "number" && Number.isFinite(value)) ||
            typeof value === "boolean")
        )
          safe[key] = value;
      }
      send(event, safe);
    },
    page(section) {
      if (
        typeof section !== "string" ||
        !/^[a-zA-Z0-9_/-]{1,120}$/.test(section)
      )
        return;
      if (lastPage === section) return;
      lastPage = section;
      send("$pageview", { section });
    },
    identify(id, properties = {}) {
      if (
        !enabled() ||
        !/^(dashboard-user:|wallet:|keychain-owner:)[a-zA-Z0-9_-]{1,128}$/.test(
          id,
        )
      )
        return;
      try {
        let state = read();
        if (
          (person && person !== id) ||
          (state.person && state.person !== id)
        ) {
          // Never merge two authenticated principals through a shared browser.
          state = fresh();
          write(state);
          account = undefined;
        }
        person = id;
        const set = {};
        if (
          typeof properties.email === "string" &&
          properties.email.length <= 254
        )
          set.email = properties.email;
        if (state.person !== id)
          send("$identify", {
            distinct_id: id,
            $anon_distinct_id: state.anonymous,
            $is_identified: true,
            $set: set,
          });
        state.person = id;
        write(state);
      } catch {
        /* Identity tracking is not authentication. */
      }
    },
    group(id) {
      if (!/^(dashboard:0x[0-9a-f]{40}|keychain:[0-9a-f]{64})$/.test(id))
        return;
      if (account === id) return;
      account = id;
      send("$groupidentify", {
        $group_type: "billing_account",
        $group_key: id,
        $group_set: { name: id, product },
        $set: { [`${product}_billing_account`]: id },
      });
      api.capture("billing_account_linked");
    },
    reset() {
      person = undefined;
      account = undefined;
      lastPage = undefined;
      if (enabled()) {
        try {
          write(fresh());
        } catch {
          memory = undefined;
        }
      }
    },
  };
  return api;
}
