export interface Analytics {
  capture(
    event: string,
    properties?: Record<string, string | number | boolean>,
  ): void;
  page(section: string): void;
  identify(id: string, properties?: { email?: string }): void;
  group(id: string): void;
  reset(): void;
}
export function createAnalytics(
  product: "marketing" | "dashboard" | "keychain",
  browser?: typeof globalThis,
): Analytics;
