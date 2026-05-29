import { NextResponse } from "next/server";
const { setKillSwitch } = require("../../../lib/vault");

export const dynamic = "force-dynamic";
export const runtime = "nodejs";

// SECURITY: this route signs a transaction with OWNER_PRIVATE_KEY and has no
// authentication. Anyone who can reach it can flip the kill switch (engage =
// stop the solver, resume = undo a safety stop). It is therefore OFF by
// default and only enabled when DASHBOARD_ENABLE_WRITES=true, which you should
// set ONLY when the dashboard is bound to localhost or a trusted network.
// Before exposing this publicly, put real auth in front of it.
export async function POST(req) {
  if (process.env.DASHBOARD_ENABLE_WRITES !== "true") {
    return NextResponse.json(
      { error: "writes disabled — set DASHBOARD_ENABLE_WRITES=true (localhost/trusted only)" },
      { status: 403 }
    );
  }
  try {
    const { on } = await req.json();
    const result = await setKillSwitch(on);
    return NextResponse.json({ ok: true, ...result });
  } catch (err) {
    return NextResponse.json({ error: err.message }, { status: 500 });
  }
}
