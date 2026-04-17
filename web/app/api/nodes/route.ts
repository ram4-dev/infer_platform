import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

export async function GET() {
  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/nodes`, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      // No caching — nodes list must always be fresh
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/nodes] Gateway returned ${res.status}`);
      return NextResponse.json({ data: [], total: 0 }, { status: 200 });
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    console.error("[api/nodes] Failed to reach gateway:", err);
    return NextResponse.json({ data: [], total: 0 }, { status: 200 });
  }
}
