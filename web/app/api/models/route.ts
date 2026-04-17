import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_API_KEY = process.env.GATEWAY_API_KEY ?? "";

export async function GET() {
  if (!GATEWAY_API_KEY) {
    return NextResponse.json(
      { error: "GATEWAY_API_KEY not configured" },
      { status: 500 }
    );
  }

  let res: Response;
  try {
    res = await fetch(`${GATEWAY_URL}/v1/models`, {
      headers: { Authorization: `Bearer ${GATEWAY_API_KEY}` },
      next: { revalidate: 60 },
    });
  } catch (err) {
    console.error("[api/models] Failed to reach gateway:", err);
    return NextResponse.json({ object: "list", data: [] }, { status: 200 });
  }

  if (!res.ok) {
    console.error(`[api/models] Gateway returned ${res.status}`);
    return NextResponse.json({ object: "list", data: [] }, { status: 200 });
  }

  const data = await res.json();
  return NextResponse.json(data);
}
