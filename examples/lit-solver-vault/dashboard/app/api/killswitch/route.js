import { NextResponse } from "next/server";
const { setKillSwitch } = require("../../../lib/vault");

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

export async function POST(req) {
  try {
    const { on } = await req.json();
    const result = await setKillSwitch(on);
    return NextResponse.json({ ok: true, ...result });
  } catch (err) {
    return NextResponse.json({ error: err.message }, { status: 500 });
  }
}
