import { NextResponse } from "next/server";

const GATEWAY_URL = process.env.GATEWAY_URL ?? "http://localhost:8080";
const GATEWAY_INTERNAL_KEY =
  process.env.GATEWAY_INTERNAL_KEY ?? "internal_dev_secret";

export async function DELETE(
  _request: Request,
  { params }: { params: Promise<{ id: string }> }
) {
  const { id } = await params;

  try {
    const res = await fetch(`${GATEWAY_URL}/v1/internal/keys/${id}`, {
      method: "DELETE",
      headers: { Authorization: `Bearer ${GATEWAY_INTERNAL_KEY}` },
    });

    if (res.status === 204) {
      return new NextResponse(null, { status: 204 });
    }

    const data = await res.json();
    return NextResponse.json(data, { status: res.status });
  } catch (err) {
    console.error("[api/keys/[id]] Failed to revoke key:", err);
    return NextResponse.json({ error: "Gateway unreachable" }, { status: 503 });
  }
}
