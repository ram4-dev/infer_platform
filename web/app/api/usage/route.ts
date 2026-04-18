import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

const EMPTY = { by_key: [], totals: { request_count: 0, total_tokens_in: 0, total_tokens_out: 0, total_tokens: 0 } };

export async function GET() {
  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/usage`, {
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
      cache: "no-store",
    });

    if (!res.ok) {
      console.error(`[api/usage] Gateway returned ${res.status}`);
      return NextResponse.json(EMPTY, { status: 200 });
    }

    return NextResponse.json(await res.json());
  } catch (err) {
    console.error("[api/usage] Failed to reach gateway:", err);
    return NextResponse.json(EMPTY, { status: 200 });
  }
}
