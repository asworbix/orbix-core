// Echo connector module - Phase 2 acceptance fixture.
//
// Wire protocol (NDJSON over stdin/stdout):
//   ← {"id": "<req-id>", "action": "ping", "inputs": {...}}
//   → {"id": "<req-id>", "success": true, "output": {...}}
//
// The supervisor enforces network/env restrictions via Deno flags before
// this code runs. Inside the module we only need to read lines, dispatch
// to the action handler, and write one response line per request.

const decoder = new TextDecoder();
const encoder = new TextEncoder();

type WireRequest = { id: string; action: string; inputs: unknown };

type Handler = (inputs: any) => Promise<unknown> | unknown;

const actions: Record<string, Handler> = {
  // Echoes inputs back. Adds a server-side timestamp so a caller can tell
  // a cached result (no new timestamp) from a fresh one. Useful for the
  // bus's idempotency acceptance test.
  ping: (inputs) => ({
    echoed: inputs ?? null,
    pong: true,
    server_time: new Date().toISOString(),
  }),
};

async function write(line: string): Promise<void> {
  await Deno.stdout.write(encoder.encode(line + "\n"));
}

async function reply(id: string, success: boolean, body: unknown): Promise<void> {
  const wire = success
    ? { id, success: true, output: body }
    : { id, success: false, ...(body as object) };
  await write(JSON.stringify(wire));
}

async function main(): Promise<void> {
  const buf = new Uint8Array(64 * 1024);
  let carry = "";
  while (true) {
    const n = await Deno.stdin.read(buf);
    if (n === null) break;
    carry += decoder.decode(buf.subarray(0, n));
    let nl = carry.indexOf("\n");
    while (nl !== -1) {
      const line = carry.slice(0, nl).trim();
      carry = carry.slice(nl + 1);
      if (line.length > 0) {
        try {
          const req = JSON.parse(line) as WireRequest;
          const handler = actions[req.action];
          if (!handler) {
            await reply(req.id, false, {
              error_code: "no_action",
              error_message: `no handler for '${req.action}'`,
            });
          } else {
            try {
              const output = await handler(req.inputs);
              await reply(req.id, true, output);
            } catch (err) {
              await reply(req.id, false, {
                error_code: "module_error",
                error_message: String(err),
              });
            }
          }
        } catch (err) {
          // Malformed line - report under a synthetic id so the supervisor
          // sees something correlated. The runtime won't accept it (id
          // mismatch) but the stderr trace makes the failure visible.
          await write(JSON.stringify({
            id: "parse-error",
            success: false,
            error_code: "parse_error",
            error_message: String(err),
          }));
        }
      }
      nl = carry.indexOf("\n");
    }
  }
}

main().catch((err) => {
  console.error("[echo] fatal:", err);
  Deno.exit(1);
});
