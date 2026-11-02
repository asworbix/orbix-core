import { defineConnector, OrbixError } from "./orbix-sdk.ts";

defineConnector({
  ping: ({ inputs }) => ({
    echoed: inputs ?? null,
    pong: true,
    server_time: new Date().toISOString(),
  }),

  fail: () => {
    // The bus surfaces `error_code: "forced_failure"` (declared in the
    // manifest), not the generic "module_error".
    throw new OrbixError("forced_failure", "this action is meant to fail");
  },
});
