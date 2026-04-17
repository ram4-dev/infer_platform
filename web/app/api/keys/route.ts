import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

export async function GET() {
  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/keys`, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/keys] Gateway returned ${res.status}`);
      return NextResponse.json({ data: [], total: 0 }, { status: 200 });
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    console.error("[api/keys] Failed to reach gateway:", err);
    return NextResponse.json({ data: [], total: 0 }, { status: 200 });
  }
}

export async function POST(request: Request) {
  let body: unknown;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: "Invalid JSON" }, { status: 400 });
  }

  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/keys`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
    });

    const data = await res.json();
    return NextResponse.json(data, { status: res.status });
  } catch (err) {
    console.error("[api/keys] Failed to create key:", err);
    return NextResponse.json({ error: "Gateway unreachable" }, { status: 503 });
  }
}
