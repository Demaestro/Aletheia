const baseUrl = (process.env.ALETHEIA_VECTOR_KB_URL || "http://127.0.0.1:47618").replace(/\/+$/, "");

async function requestJson(path, options = {}) {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 2500);
  try {
    const response = await fetch(`${baseUrl}${path}`, {
      ...options,
      signal: controller.signal,
    });
    const text = await response.text();
    let json;
    try {
      json = JSON.parse(text);
    } catch {
      throw new Error(`${path} returned non-JSON response: ${text.slice(0, 160)}`);
    }
    if (!response.ok) {
      throw new Error(`${path} failed with HTTP ${response.status}: ${text.slice(0, 240)}`);
    }
    return json;
  } finally {
    clearTimeout(timeout);
  }
}

const health = await requestJson("/health");
if (!health.ok || !Array.isArray(health.translations) || !health.translations.includes("kjv")) {
  throw new Error(`Vector KB unhealthy: ${JSON.stringify(health)}`);
}

const search = await requestJson("/search", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({
    query: "the lord is my shepherd i have everything i need",
    translationId: "kjv",
    limit: 3,
    minScore: 0.1,
  }),
});

const first = search.results?.[0];
if (!first || first.book !== "Psalm" || first.chapter !== 23 || first.verse !== 1) {
  throw new Error(`Vector KB returned wrong top result: ${JSON.stringify(search)}`);
}

console.log(JSON.stringify({
  ok: true,
  translations: health.translations.length,
  latencyMs: search.latencyMs,
  topResult: first.reference,
}, null, 2));
