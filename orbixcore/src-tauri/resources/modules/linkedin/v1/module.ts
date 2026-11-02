// LinkedIn connector module - reference shape for an API connector
// using the Orbix SDK shim. ~80 lines of business logic; the SDK
// handles NDJSON framing, error classification, and stdin/stdout
// lifecycle.
//
// The manifest pins network access to api.linkedin.com + www.linkedin.com,
// declares LINKEDIN_TOKEN as the only readable env var, and maps the
// vault's `access_token` key into that env var at spawn time. The
// static scanner verifies every fetch host in this file matches the
// manifest's host_allowlist before activation - every fetch URL must
// therefore be a literal string at the call site (no variable URLs).

import { defineConnector, env, OrbixError } from "./orbix-sdk.ts";

function classifyHttp(status: number): never {
  if (status === 401 || status === 403) {
    throw new OrbixError("auth_expired", `LinkedIn returned HTTP ${status}`);
  }
  if (status === 429) {
    throw new OrbixError("rate_limited", "LinkedIn rate limit hit");
  }
  throw new OrbixError("upstream_error", `LinkedIn HTTP ${status}`);
}

function authHeaders(): Record<string, string> {
  return { Authorization: `Bearer ${env("LINKEDIN_TOKEN")}` };
}

function postHeaders(): Record<string, string> {
  return {
    ...authHeaders(),
    "Content-Type": "application/json",
    "X-Restli-Protocol-Version": "2.0.0",
  };
}

function ugcPostBody(text: string, visibility: string): any {
  return {
    lifecycleState: "PUBLISHED",
    specificContent: {
      "com.linkedin.ugc.ShareContent": {
        shareCommentary: { text },
        shareMediaCategory: "NONE",
      },
    },
    visibility: {
      "com.linkedin.ugc.MemberNetworkVisibility": visibility,
    },
  };
}

defineConnector({
  // Cheap liveness probe - used by Bob's authoring loop §8 step 5 as
  // the smoke test against a real access token.
  userinfo: async () => {
    const r = await fetch("https://api.linkedin.com/v2/userinfo", {
      headers: authHeaders(),
    });
    if (!r.ok) classifyHttp(r.status);
    const me = await r.json();
    return { sub: me.sub, name: me.name, email: me.email };
  },

  // Compose a payload without submitting. The bus + scheduler use this
  // when the JobProfile review gate is Always or FirstN. The payload
  // round-trips through the approval UI; on approval, the bus calls
  // `post` with the same body.
  draft_post: ({ inputs }) => {
    const { text, visibility = "PUBLIC" } = inputs as {
      text: string;
      visibility?: string;
    };
    if (!text || text.trim().length === 0) {
      throw new OrbixError("invalid_input", "text is required");
    }
    return {
      action: "post",
      body: ugcPostBody(text, visibility),
    };
  },

  post: async ({ inputs }) => {
    const { text, visibility = "PUBLIC" } = inputs as {
      text: string;
      visibility?: string;
    };
    if (!text || text.trim().length === 0) {
      throw new OrbixError("invalid_input", "text is required");
    }

    // userinfo first - we need the author URN.
    const meRes = await fetch("https://api.linkedin.com/v2/userinfo", {
      headers: authHeaders(),
    });
    if (!meRes.ok) classifyHttp(meRes.status);
    const me = await meRes.json();

    const body = ugcPostBody(text, visibility);
    body.author = `urn:li:person:${me.sub}`;

    const postRes = await fetch("https://api.linkedin.com/v2/ugcPosts", {
      method: "POST",
      headers: postHeaders(),
      body: JSON.stringify(body),
    });
    if (!postRes.ok) classifyHttp(postRes.status);
    const result = await postRes.json();
    return { id: result.id ?? null };
  },
});
