export async function jsonFetch(
  url: string,
  init: RequestInit = {},
  timeoutMs = 15000,
  maxBytes = 1024 * 1024,
): Promise<any> {
  const controller = new AbortController();
  const abort = () => controller.abort(init.signal?.reason);
  if (init.signal?.aborted) abort();
  else init.signal?.addEventListener("abort", abort, { once: true });
  const timer = setTimeout(
    () => controller.abort(new Error("Request timed out")),
    timeoutMs,
  );
  let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
  try {
    const res = await fetch(url, {
      ...init,
      redirect: "error",
      credentials: init.credentials ?? "omit",
      signal: controller.signal,
    });
    if (!res.ok) throw new Error(`Request failed (${res.status})`);
    if (Number(res.headers.get("content-length")) > maxBytes)
      throw new Error("Response too large");
    if (!res.body) throw new Error("Missing response");
    reader = res.body.getReader();
    const chunks: Uint8Array[] = [];
    let size = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > maxBytes) throw new Error("Response too large");
      chunks.push(value);
    }
    const bytes = new Uint8Array(size);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } finally {
    clearTimeout(timer);
    init.signal?.removeEventListener("abort", abort);
    if (reader) {
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    controller.abort();
  }
}
