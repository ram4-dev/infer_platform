import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

export async function GET() {
  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/provider/stats`, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/provider] Gateway returned ${res.status}`);
      return NextResponse.json(
        { nodes: [], totals: { node_count: 0, request_count_7d: 0, tokens_served_7d: 0, estimated_earnings_usd_7d: 0 } },
        { status: 200 }
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    console.error("[api/provider] Failed to reach gateway:", err);
    return NextResponse.json(
      { nodes: [], totals: { node_count: 0, request_count_7d: 0, tokens_served_7d: 0, estimated_earnings_usd_7d: 0 } },
      { status: 200 }
    );
  }
}
