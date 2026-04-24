// @ts-nocheck
export async function getJson(url: string, init: any = {}) {
  const response = await fetch(url, init);
  const text = await response.text();
  let body: any = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = text;
  }
  return { ok: response.ok, status: response.status, body, text };
}

export async function ollamaTags(ollamaUrl: string) {
  return getJson(`${ollamaUrl}/api/tags`);
}

export async function pullModel(ollamaUrl: string, model: string) {
  const response = await fetch(`${ollamaUrl}/api/pull`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ model, stream: false }),
  });
  const text = await response.text();
  if (!response.ok) {
    throw new Error(`Ollama pull failed (${response.status}): ${text}`);
  }
  return text;
}
