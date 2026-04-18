import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

const EMPTY = {
  total_requests: 0,
  total_tokens_in: 0,
  total_tokens_out: 0,
  total_tokens: 0,
  total_spend_usd: 0,
  tokens_by_model: [],
  daily_spend: [],
};

export async function GET(request: Request) {
  const qs = new URL(request.url).search;
  const url = `${GATEWAY_URL}/v1/internal/analytics/consumer${qs}`;

  try {
    const res = await fetch(url, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/consumer] Gateway returned ${res.status}`);
      return NextResponse.json(EMPTY, { status: 200 });
    }

    return NextResponse.json(await res.json());
  } catch (err) {
    console.error("[api/consumer] Failed to reach gateway:", err);
    return NextResponse.json(EMPTY, { status: 200 });
  }
}
