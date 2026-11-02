// ─────────────────────────────────────────────
//  Orbix Connector SDK - shim modules import to skip NDJSON wrangling.
//
//  A module's module.ts becomes one tiny function per action:
//
//      import { defineConnector, env, OrbixError } from "./_sdk/orbix-sdk.ts";
//      defineConnector({
//        async post({ inputs }) {
//          const tok = env("LINKEDIN_TOKEN");
//          const r = await fetch("https://api.linkedin.com/v2/posts", {
//            method: "POST",
//            headers: { Authorization: `Bearer ${tok}` },
//            body: JSON.stringify(inputs),
//          });
//          if (r.status === 401) throw new OrbixError("auth_expired", "401");
//          if (!r.ok) throw new OrbixError("upstream_error", `HTTP ${r.status}`);
//          return await r.json();
//        },
//      });
//
//  Wire protocol matches connector_runtime.rs:
//      ← {"id": "<req-id>", "action": "<name>", "inputs": <json>}
//      → {"id": "<req-id>", "success": true,  "output": <json>}
//      → {"id": "<req-id>", "success": false, "error_code": "<code>",
//          "error_message": "<msg>"}
//
//  The shim handles:
//    - stdin line parsing (NDJSON, with carry across reads)
//    - response framing
//    - error classification - anything thrown becomes a typed error
//      with a sensible default code if it isn't an OrbixError
//    - serial dispatch (one job at a time; the bus enforces per-slug
//      concurrency upstream)
// ─────────────────────────────────────────────

export type ActionContext<I> = { inputs: I; action: string };

export type Action<I = unknown, O = unknown> = (
  ctx: ActionContext<I>,
) => Promise<O> | O;

export type ConnectorSpec = {
  [actionName: string]: Action<any, any>;
};

export class OrbixError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

/// Read an env var. Throws OrbixError("missing_env", ...) if unset so
/// the bus surfaces a structured error rather than an opaque undefined.
export function env(name: string): string {
  const v = Deno.env.get(name);
  if (v === undefined || v === "") {
    throw new OrbixError("missing_env", `env var ${name} is not set`);
  }
  return v;
}

const decoder = new TextDecoder();
const encoder = new TextEncoder();

async function writeLine(obj: unknown): Promise<void> {
  await Deno.stdout.write(encoder.encode(JSON.stringify(obj) + "\n"));
}

/// Write a value back into the connector vault. The supervisor scopes
/// the slug to this module - no other connector can be reached.
/// Only the standard OAuth-refresh keys are accepted on the supervisor
/// side: access_token, refresh_token, expires_at_unix.
export async function vaultPut(
  key: "access_token" | "refresh_token" | "expires_at_unix",
  value: string,
): Promise<void> {
  await writeLine({ op: "vault_put", key, value });
}

/// Run an OAuth2 refresh_token grant against `tokenUrl`, write the
/// new access_token (and refresh_token, if rotated) into the vault,
/// and return the parsed response. Modules call this on a 401 from
/// the provider and then retry the original request with the new
/// LINKEDIN_TOKEN / GOOGLE_TOKEN / etc. that env() will now return on
/// the NEXT process restart - the running process keeps its already-
/// loaded env, so the typical pattern is: refresh, throw a typed
/// auth_expired, and let the supervisor restart pick up the new token.
export async function refreshOauth(opts: {
  tokenUrl: string;
  refreshToken: string;
  clientId: string;
  clientSecret?: string;
}): Promise<{ access_token: string; refresh_token?: string; expires_in?: number }> {
  const params = new URLSearchParams();
  params.set("grant_type", "refresh_token");
  params.set("refresh_token", opts.refreshToken);
  params.set("client_id", opts.clientId);
  if (opts.clientSecret) params.set("client_secret", opts.clientSecret);

  const r = await fetch(opts.tokenUrl, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: params.toString(),
  });
  if (!r.ok) {
    throw new OrbixError("refresh_failed", `token refresh HTTP ${r.status}`);
  }
  const body = await r.json() as {
    access_token: string;
    refresh_token?: string;
    expires_in?: number;
  };
  await vaultPut("access_token", body.access_token);
  if (body.refresh_token) await vaultPut("refresh_token", body.refresh_token);
  if (typeof body.expires_in === "number") {
    const exp = Math.floor(Date.now() / 1000) + body.expires_in;
    await vaultPut("expires_at_unix", String(exp));
  }
  return body;
}

type WireRequest = { id: string; action: string; inputs: unknown };

function classify(err: unknown): { code: string; message: string } {
  if (err instanceof OrbixError) {
    return { code: err.code, message: err.message };
  }
  if (err instanceof Error) {
    // Network errors, JSON parse errors, etc. The bus already records
    // the stack via stderr; the wire response stays terse.
    return { code: "module_error", message: err.message };
  }
  return { code: "module_error", message: String(err) };
}

export function defineConnector(actions: ConnectorSpec): void {
  // Note: top-level await is fine in Deno. We return a promise the
  // runtime keeps alive for the lifetime of the subprocess.
  (async () => {
    const buf = new Uint8Array(64 * 1024);
    let carry = "";
    while (true) {
      const n = await Deno.stdin.read(buf);
      if (n === null) break; // EOF - supervisor closed our stdin
      carry += decoder.decode(buf.subarray(0, n));
      let nl = carry.indexOf("\n");
      while (nl !== -1) {
        const line = carry.slice(0, nl).trim();
        carry = carry.slice(nl + 1);
        if (line.length > 0) await dispatch(line, actions);
        nl = carry.indexOf("\n");
      }
    }
  })().catch((e) => {
    // If the loop itself blows up, exit non-zero so the supervisor
    // counts a crash and restarts with backoff.
    console.error("[orbix-sdk] fatal:", e);
    Deno.exit(1);
  });
}

async function dispatch(line: string, actions: ConnectorSpec): Promise<void> {
  let req: WireRequest;
  try {
    req = JSON.parse(line) as WireRequest;
  } catch (e) {
    // Malformed input. The runtime will not get an id match, but emit
    // something correlated so stderr trace is non-empty.
    await writeLine({
      id: "parse-error",
      success: false,
      error_code: "parse_error",
      error_message: String(e),
    });
    return;
  }

  const handler = actions[req.action];
  if (!handler) {
    await writeLine({
      id: req.id,
      success: false,
      error_code: "no_action",
      error_message: `no handler for '${req.action}'`,
    });
    return;
  }

  try {
    const output = await handler({ inputs: req.inputs, action: req.action });
    await writeLine({ id: req.id, success: true, output: output ?? null });
  } catch (err) {
    const { code, message } = classify(err);
    await writeLine({
      id: req.id,
      success: false,
      error_code: code,
      error_message: message,
    });
  }
}
