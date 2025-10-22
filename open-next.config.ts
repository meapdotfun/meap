// @ts-nocheck
// open-next.config.ts
export default {
  default: {
    override: {
      wrapper: "cloudflare-node",
      converter: "edge",
      proxyExternalRequest: "fetch",
      incrementalCache: "dummy",
      tagCache: "dummy",
      queue: "dummy",
    },
  },
  edgeExternals: ["node:crypto"],
  middleware: {
    external: true,
    override: {
      wrapper: "cloudflare-edge",
      converter: "edge",
      proxyExternalRequest: "fetch",
      incrementalCache: "dummy",
      tagCache: "dummy",
      queue: "dummy",
    },
  },
  bindings: {
    // IMPORTANT: object, not string — uses your wrangler.jsonc binding name
    kvNamespaces: [{ binding: "MEAP_KV" }]
  },
  dangerous: { enableCacheInterception: false },
};