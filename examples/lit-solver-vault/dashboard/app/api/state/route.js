import { NextResponse } from "next/server";
const { getState } = require("../../../lib/vault");

// Always fresh — this is a live ops view.
export const dynamic = "force-dynamic";
export const runtime = "nodejs";

export async function GET() {
  try {
    const state = await getState();
    return NextResponse.json(state);
  } catch (err) {
    return NextResponse.json({ error: err.message }, { status: 500 });
  }
}
