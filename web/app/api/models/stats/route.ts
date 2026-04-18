import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

const EMPTY = { models: [], price_per_m_tokens: 1.0 };

export async function GET() {
  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/models/stats`, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/models/stats] Gateway returned ${res.status}`);
      return NextResponse.json(EMPTY, { status: 200 });
    }

    return NextResponse.json(await res.json());
  } catch (err) {
    console.error("[api/models/stats] Failed to reach gateway:", err);
    return NextResponse.json(EMPTY, { status: 200 });
  }
}
