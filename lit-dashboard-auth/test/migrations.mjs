import { readdir, readFile } from "node:fs/promises";
const directory = new URL("../migrations/", import.meta.url);

// Match Wrangler: apply each migration as one transaction, including PRAGMAs.
export async function applyMigrations(db, files) {
  files ??= (await readdir(directory)).filter(name => name.endsWith(".sql")).sort();
  for (const file of files) {
    const sql = await readFile(new URL(file, directory), "utf8");
    await db.batch(sql.split(";").map(s => s.trim()).filter(Boolean).map(s => db.prepare(s)));
  }
}
