/**
 * Minimal ustar tar builder for k6.
 *
 * The `/lit_binary_action` endpoint takes a base64-encoded tar(.gz) bundle
 * (see litApiServer.ts). k6's JS runtime (goja) has no tar/gzip library, so
 * this packs a handful of small ASCII text files into a plain (uncompressed)
 * ustar archive by hand — enough to ship a gVisor action bundle. The server
 * derives the content id from the decoded bytes, so no checksum is sent.
 *
 * Kept intentionally small: regular files only, ASCII content only (one byte
 * per character), deterministic (zeroed mtime/uid/gid) so identical inputs
 * hash to the same CID the runner caches under.
 */
import encoding from "k6/encoding";

export interface BundleFile {
  /** Path within the bundle root, e.g. "startup.sh". */
  name: string;
  /** File contents. ASCII only — one byte per character. */
  content: string;
  /** Octal file mode. Defaults to 0o644 (0o755 for e.g. an entrypoint). */
  mode?: number;
}

const BLOCK = 512;

function writeAscii(buf: Uint8Array, offset: number, str: string): void {
  for (let i = 0; i < str.length; i++) {
    buf[offset + i] = str.charCodeAt(i) & 0xff;
  }
}

/** Zero-padded octal of `len - 1` digits followed by a NUL, into `len` bytes. */
function writeOctal(
  buf: Uint8Array,
  offset: number,
  len: number,
  value: number,
): void {
  const digits = len - 1;
  let s = value.toString(8);
  while (s.length < digits) s = "0" + s;
  writeAscii(buf, offset, s);
  buf[offset + digits] = 0;
}

function header(file: BundleFile, size: number): Uint8Array {
  if (file.name.length > 100) {
    throw new Error(`gvisorBundle: filename too long for ustar header (${file.name.length} > 100): ${file.name}`);
  }
  for (let i = 0; i < file.name.length; i++) {
    const c = file.name.charCodeAt(i);
    if (c === 0 || c > 0x7f) {
      throw new Error(`gvisorBundle: filename must be ASCII without NUL bytes: ${file.name}`);
    }
  }

  const h = new Uint8Array(BLOCK);
  writeAscii(h, 0, file.name); // name[100]
  writeOctal(h, 100, 8, file.mode ?? 0o644); // mode[8]
  writeOctal(h, 108, 8, 0); // uid[8]
  writeOctal(h, 116, 8, 0); // gid[8]
  writeOctal(h, 124, 12, size); // size[12]
  writeOctal(h, 136, 12, 0); // mtime[12] — zeroed for determinism
  writeAscii(h, 148, "        "); // chksum[8] — spaces while summing
  h[156] = "0".charCodeAt(0); // typeflag '0' = regular file
  writeAscii(h, 257, "ustar"); // magic[6] = "ustar\0"
  h[262] = 0;
  writeAscii(h, 263, "00"); // version[2]

  // Header checksum: sum of every header byte (chksum field counted as
  // spaces), stored as 6 octal digits + NUL + space (the traditional format
  // the Rust `tar` crate and every extractor accept).
  let sum = 0;
  for (let i = 0; i < BLOCK; i++) sum += h[i];
  let cs = sum.toString(8);
  while (cs.length < 6) cs = "0" + cs;
  writeAscii(h, 148, cs);
  h[154] = 0;
  h[155] = 0x20;
  return h;
}

/** Build a plain (uncompressed) ustar archive of `files`. */
export function buildTar(files: BundleFile[]): Uint8Array {
  const blocks: Uint8Array[] = [];
  for (const f of files) {
    // ASCII only: content.length is used as the tar byte size, and the CID the
    // server derives depends on exact bytes. A non-ASCII char would make length
    // (chars) disagree with the UTF-8 byte count and corrupt the archive.
    for (let i = 0; i < f.content.length; i++) {
      if (f.content.charCodeAt(i) > 0x7f) {
        throw new Error(
          `gvisorBundle: ${f.name} has a non-ASCII byte at index ${i}; bundle files must be ASCII-only`,
        );
      }
    }
    const size = f.content.length; // ASCII → byte length
    blocks.push(header(f, size));
    const padded = Math.ceil(size / BLOCK) * BLOCK;
    const data = new Uint8Array(padded);
    writeAscii(data, 0, f.content);
    blocks.push(data);
  }
  // Two zero blocks mark end-of-archive.
  blocks.push(new Uint8Array(BLOCK * 2));

  const total = blocks.reduce((n, b) => n + b.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const b of blocks) {
    out.set(b, offset);
    offset += b.length;
  }
  return out;
}

/** Build the archive of `files` and base64-encode it for the request body. */
export function buildBundleBase64(files: BundleFile[]): string {
  return encoding.b64encode(buildTar(files).buffer);
}
