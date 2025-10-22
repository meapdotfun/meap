import { keccak_256 } from 'js-sha3';
import * as secp from '@noble/secp256k1';

export interface Env {
  MEAP_KV: KVNamespace;
  OPENAI_API_KEY?: string;
  ASTER_PRIVATE_KEY?: string;
  ASTER_API_BASE?: string;
  ADMIN_KEY?: string;
  QWEN_API_KEY?: string;
  QWEN_BASE_URL?: string; // default: https://dashscope.aliyuncs.com/compatible-mode/v1
}

function cors(headers: Record<string, string> = {}) {
  return {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET,POST,OPTIONS",
    "Access-Control-Allow-Headers": "*",
    ...headers,
  };
}

// ---------- Aster signing helpers ----------
function hexToBytes(hex: string): Uint8Array {
  const h = hex.startsWith('0x') ? hex.slice(2) : hex;
  const out = new Uint8Array(h.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(h.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function bytesToHex(bytes: Uint8Array): string {
  return '0x' + Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
}

async function sha256Hex(input: string): Promise<string> {
  const data = new TextEncoder().encode(input);
  const buf = await crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(buf)).map(b => b.toString(16).padStart(2, '0')).join('');
}

function evmAddressFromPrivateKey(privHex: string): `0x${string}` {
  const priv = privHex.startsWith('0x') ? privHex.slice(2) : privHex;
  const pub = secp.getPublicKey(priv, false); // uncompressed 65 bytes, 0x04 + X(32) + Y(32)
  const pubNoPrefix = pub.slice(1);
  const hashHex = keccak_256(pubNoPrefix); // hex
  const addr = '0x' + hashHex.slice(-40);
  return addr as `0x${string}`;
}

async function personalSign(privHex: string, message: string): Promise<string> {
  const msgBytes = new TextEncoder().encode(message);
  const prefix = new TextEncoder().encode(`\x19Ethereum Signed Message:\n${msgBytes.length}`);
  const prefixed = new Uint8Array(prefix.length + msgBytes.length);
  prefixed.set(prefix, 0);
  prefixed.set(msgBytes, prefix.length);
  const digestHex = keccak_256(prefixed);
  const digest = hexToBytes('0x' + digestHex);
  const [sig, recId] = await secp.sign(digest, privHex.startsWith('0x') ? privHex.slice(2) : privHex, { recovered: true, der: false });
  const v = 27 + recId;
  const full = new Uint8Array(65);
  full.set(sig, 0);
  full[64] = v;
  return bytesToHex(full);
}

async function asterRequest(env: Env, method: string, path: string, body?: any): Promise<Response> {
  const base = (env.ASTER_API_BASE || '').trim().replace(/\/+$/, '');
  const priv = env.ASTER_PRIVATE_KEY || '';
  if (!base) throw new Error('ASTER base missing');
  const addr = priv ? evmAddressFromPrivateKey(priv) : (undefined as any);
  const ts = Math.floor(Date.now() / 1000); // seconds
  const m = method.toUpperCase();
  const p = path.startsWith('/') ? path : `/${path}`;
  const bodyText = body === undefined || body === null ? '' : (typeof body === 'string' ? body : JSON.stringify(body));
  const bodySha256 = await sha256Hex(bodyText);
  const canonical = `${ts}\n${m}\n${p}\n${bodySha256}`;
  // Helper to send one request with given url path
  const send = async (urlPath: string, headers: Record<string,string>): Promise<Response> => {
    return fetch(`${base}${urlPath}`, { method: m, headers, body: bodyText || undefined });
  };

  // Try EVM-sign first if wallet key is present
  if (priv) {
    try {
      const sig = await personalSign(priv, canonical);
      const headers: Record<string, string> = {
        'content-type': 'application/json',
        'x-aster-address': addr,
        'x-aster-timestamp': String(ts),
        'x-aster-signature': sig,
      };
      // try primary path
      let r = await send(p, headers);
      if (r.ok || r.status !== 404) return r;
      // try common variants if 404
      // /v1/futures/X -> /fapi/v1/X
      const alt1 = p.startsWith('/v1/futures') ? '/fapi' + p.replace('/v1/futures', '/v1') : null;
      if (alt1) { r = await send(alt1, headers); if (r.ok || r.status !== 404) return r; }
      // /v1/futures/X -> /api/v1/futures/X
      const alt2 = p.startsWith('/v1') ? '/api' + p : null;
      if (alt2) { r = await send(alt2, headers); if (r.ok || r.status !== 404) return r; }
      // positions specific fallbacks
      if (p.includes('/positions')) {
        const alt3 = '/fapi/v1/positionRisk';
        r = await send(alt3, headers); if (r.ok || r.status !== 404) return r;
        const alt4 = '/fapi/v2/positionRisk';
        r = await send(alt4, headers); if (r.ok || r.status !== 404) return r;
      }
      // account specific fallback
      if (p.includes('/account')) {
        const alt5 = '/fapi/v1/account';
        r = await send(alt5, headers); if (r.ok || r.status !== 404) return r;
      }
      // if API key exists, fall through to API-key auth
      if (!env.ASTER_API_KEY || !env.ASTER_API_SECRET) return r;
      // fall through to API-key if provided
    } catch {}
  }
  // API-key fallback if configured
  if (env.ASTER_API_KEY && env.ASTER_API_SECRET) {
    const apiKey = env.ASTER_API_KEY;
    const apiSecret = env.ASTER_API_SECRET;
    const sigApi = await hmacHex('SHA-256', apiSecret, canonical);
    const headers: Record<string, string> = {
      'content-type': 'application/json',
      'x-aster-key': apiKey,
      'x-aster-timestamp': String(ts),
      'x-aster-signature': sigApi,
    };
    // try primary path
    let r = await send(p, headers);
    if (r.ok || r.status !== 404) return r;
    // try common variants if 404
    const alt1 = p.startsWith('/v1/futures') ? '/fapi' + p.replace('/v1/futures', '/v1') : null;
    if (alt1) { r = await send(alt1, headers); if (r.ok || r.status !== 404) return r; }
    const alt2 = p.startsWith('/v1') ? '/api' + p : null;
    if (alt2) { r = await send(alt2, headers); if (r.ok || r.status !== 404) return r; }
    if (p.includes('/positions')) {
      const alt3 = '/fapi/v1/positionRisk';
      r = await send(alt3, headers); if (r.ok || r.status !== 404) return r;
      const alt4 = '/fapi/v2/positionRisk';
      r = await send(alt4, headers); if (r.ok || r.status !== 404) return r;
    }
    if (p.includes('/account')) {
      const alt5 = '/fapi/v1/account';
      r = await send(alt5, headers); if (r.ok || r.status !== 404) return r;
    }
    return r;
  }
  // If nothing configured, return a 400-like Response
  return new Response('ASTER auth not configured', { status: 400 });
}

async function hmacHex(alg: 'SHA-256', secret: string, message: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    'raw', new TextEncoder().encode(secret), { name: 'HMAC', hash: alg }, false, ['sign']
  );
  const sig = await crypto.subtle.sign('HMAC', key, new TextEncoder().encode(message));
  return Array.from(new Uint8Array(sig)).map(b => b.toString(16).padStart(2,'0')).join('');
}

async function asterFapiGetJson(env: Env, path: string): Promise<any | null> {
  const base = (env.ASTER_API_BASE || '').trim().replace(/\/+$/, '');
  const apiKey = env.ASTER_API_KEY;
  const apiSecret = env.ASTER_API_SECRET;
  if (!base || !apiKey || !apiSecret) return null;
  const ts = Date.now(); // ms per Binance spec
  const recv = 5000;
  const query = `timestamp=${ts}&recvWindow=${recv}`;
  const sig = await hmacHex('SHA-256', apiSecret, query);
  const url = `${base}${path}?${query}&signature=${sig}`;
  const r = await fetch(url, { headers: { 'X-MBX-APIKEY': apiKey } });
  if (!r.ok) return null;
  return await r.json().catch(() => null);
}

async function asterFapiGetPublicJson(env: Env, pathWithQuery: string): Promise<any | null> {
  const base = (env.ASTER_API_BASE || '').trim().replace(/\/+$/, '');
  if (!base) return null;
  const r = await fetch(`${base}${pathWithQuery}`);
  if (!r.ok) return null;
  return await r.json().catch(() => null);
}

// ---------- Indicators ----------
function sma(values: number[], period: number): number[] {
  const out: number[] = [];
  let sum = 0;
  for (let i = 0; i < values.length; i++) {
    sum += values[i];
    if (i >= period) sum -= values[i - period];
    out.push(i >= period - 1 ? sum / period : NaN);
  }
  return out;
}

function rsi(values: number[], period = 14): number[] {
  const out: number[] = Array(values.length).fill(NaN);
  if (values.length < period + 1) return out;
  let gains = 0, losses = 0;
  for (let i = 1; i <= period; i++) {
    const ch = values[i] - values[i - 1];
    if (ch >= 0) gains += ch; else losses -= ch;
  }
  let avgGain = gains / period;
  let avgLoss = losses / period;
  out[period] = avgLoss === 0 ? 100 : 100 - (100 / (1 + (avgGain / avgLoss)));
  for (let i = period + 1; i < values.length; i++) {
    const ch = values[i] - values[i - 1];
    const gain = ch > 0 ? ch : 0;
    const loss = ch < 0 ? -ch : 0;
    avgGain = (avgGain * (period - 1) + gain) / period;
    avgLoss = (avgLoss * (period - 1) + loss) / period;
    out[i] = avgLoss === 0 ? 100 : 100 - (100 / (1 + (avgGain / avgLoss)));
  }
  return out;
}

async function asterFapiSignedPost(env: Env, path: string, params: Record<string, string | number>): Promise<Response> {
  const base = (env.ASTER_API_BASE || '').trim().replace(/\/+$/, '');
  const apiKey = env.ASTER_API_KEY;
  const apiSecret = env.ASTER_API_SECRET;
  if (!base || !apiKey || !apiSecret) return new Response('ASTER_API_BASE/API_KEY/API_SECRET missing', { status: 400 });
  const ts = Date.now();
  const recv = 5000;
  const qp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) qp.append(k, String(v));
  qp.append('timestamp', String(ts));
  qp.append('recvWindow', String(recv));
  const sig = await hmacHex('SHA-256', apiSecret, qp.toString());
  qp.append('signature', sig);
  const url = `${base}${path}`;
  return fetch(url, {
    method: 'POST',
    headers: { 'X-MBX-APIKEY': apiKey, 'content-type': 'application/x-www-form-urlencoded' },
    body: qp.toString()
  });
}

async function handleDebug(req: Request, env: Env) {
  const info: any = { ok: true };
  info.hasEnv = !!env.MEAP_KV;
  if (env.MEAP_KV) {
    try {
      const data = await env.MEAP_KV.get(new URL('/__events.json', req.url).toString(), { type: 'json' });
      info.kvReadable = true;
      info.eventsCount = Array.isArray(data) ? data.length : 0;
    } catch (e: any) {
      info.kvReadable = false;
      info.error = String(e?.message || e);
    }
  } else {
    info.message = 'KV binding not found in worker environment';
  }
  return new Response(JSON.stringify(info, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleEvents(req: Request, env: Env) {
  const key = new URL('/__events.json', req.url).toString();
  const data = (await env.MEAP_KV.get(key, { type: 'json' })) || [];
  return new Response(JSON.stringify({ events: data.slice(-200).reverse() }), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleAgentsGet(req: Request, env: Env) {
  const key = new URL('/__agents.json', req.url).toString();
  const data = (await env.MEAP_KV.get(key, { type: 'json' })) || [];
  return new Response(JSON.stringify({ agents: (data as any[]).slice(-100).reverse() }), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleAgentsPost(req: Request, env: Env) {
  const url = new URL(req.url);
  const keyAgents = new URL('/__agents.json', url).toString();
  const keyEvents = new URL('/__events.json', url).toString();
  const body: any = await req.json().catch(() => ({}));
  const owner = body?.owner;
  if (!owner || owner === '0x0000') {
    return new Response(JSON.stringify({ error: 'wallet_required' }), {
      status: 400,
      headers: cors({ 'Content-Type': 'application/json' })
    });
  }
  const list = ((await env.MEAP_KV.get(keyAgents, { type: 'json' })) as any[]) || [];
  // one agent per wallet: return 409 if exists
  const existing = list.find((a: any) => a?.owner?.toLowerCase?.() === owner?.toLowerCase?.());
  if (existing) {
    return new Response(JSON.stringify({ error: 'exists', agent: existing }), {
      status: 409,
      headers: cors({ 'Content-Type': 'application/json' })
    });
  }
  const agent = { id: `agent_${Date.now()}`, owner, createdAt: Date.now(), webhookUrl: body?.webhookUrl || null, hmacSecret: body?.hmacSecret || null };
  list.push(agent);
  await env.MEAP_KV.put(keyAgents, JSON.stringify(list));
  const events = ((await env.MEAP_KV.get(keyEvents, { type: 'json' })) as any[]) || [];
  events.push({ type: 'agent_created', at: Date.now(), owner, id: agent.id });
  await env.MEAP_KV.put(keyEvents, JSON.stringify(events));
  return new Response(JSON.stringify(agent), { headers: cors({ 'Content-Type': 'application/json' }), status: 201 });
}

async function handleMessage(req: Request, env: Env) {
  const url = new URL(req.url);
  const keyEvents = new URL('/__events.json', url).toString();
  const body: any = await req.json().catch(() => ({}));
  const from = body?.from || 'anon';
  const to = body?.to || 'any-agent';
  const resp = { id: `msg_${Date.now()}`, type: 'Response', from: to, to: from, payload: { ok: true, echo: body?.payload ?? null }, timestamp: new Date().toISOString() };
  const events = ((await env.MEAP_KV.get(keyEvents, { type: 'json' })) as any[]) || [];
  const kind = body?.payload?.kind;
  if (kind === 'onchain_register') {
    events.push({ type: 'onchain_register', at: Date.now(), owner: from, agentId: body?.payload?.agentId, tx: body?.payload?.tx, chain: body?.payload?.chain || 'bsc' });
  } else if (kind === 'onchain_tip') {
    events.push({ type: 'onchain_tip', at: Date.now(), tipper: from, owner: body?.payload?.owner || to, agentId: body?.payload?.agentId, amount: body?.payload?.amount, tx: body?.payload?.tx, chain: body?.payload?.chain || 'bsc' });
  } else {
    // generic message
    events.push({ type: 'message', at: Date.now(), from, to, id: resp.id });
    // also place in recipient inbox
    const keyInbox = new URL(`/inbox_${to}.json`, url).toString();
    const inbox = ((await env.MEAP_KV.get(keyInbox, { type: 'json' })) as any[]) || [];
    inbox.push({ id: resp.id, from, to, at: Date.now(), payload: body?.payload ?? null });
    await env.MEAP_KV.put(keyInbox, JSON.stringify(inbox));

    // webhook delivery if receiver has webhookUrl
    try {
      const keyAgents = new URL('/__agents.json', url).toString();
      const agents = ((await env.MEAP_KV.get(keyAgents, { type: 'json' })) as any[]) || [];
      const agent = agents.find((a: any) => a?.id === to);
      if (agent?.webhookUrl) {
        const payload = { id: resp.id, from, to, at: Date.now(), payload: body?.payload ?? null };
        const headers: Record<string,string> = { 'content-type': 'application/json' };
        // simple HMAC using crypto.subtle if secret provided
        if (agent.hmacSecret) {
          const key = await crypto.subtle.importKey(
            'raw', new TextEncoder().encode(agent.hmacSecret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']
          );
          const sigBuf = await crypto.subtle.sign('HMAC', key, new TextEncoder().encode(JSON.stringify(payload)));
          const sigHex = Array.from(new Uint8Array(sigBuf)).map(b=>b.toString(16).padStart(2,'0')).join('');
          headers['x-meap-signature'] = `sha256=${sigHex}`;
        }
        const r = await fetch(agent.webhookUrl, { method: 'POST', headers, body: JSON.stringify(payload) });
        const reply = await r.json().catch(()=>null);
        if (reply?.reply) {
          const outId = `msg_${Date.now()}_${Math.random().toString(36).slice(2)}`;
          const out = { id: outId, from: to, to: from, at: Date.now(), payload: reply.reply };
          // store reply to sender inbox and global events
          const keyInboxFrom = new URL(`/inbox_${from}.json`, url).toString();
          const fromInbox = ((await env.MEAP_KV.get(keyInboxFrom, { type: 'json' })) as any[]) || [];
          fromInbox.push(out);
          await env.MEAP_KV.put(keyInboxFrom, JSON.stringify(fromInbox));
          events.push({ type: 'message', at: Date.now(), from: to, to: from, id: out.id });
        }
      }
    } catch {}
  }
  await env.MEAP_KV.put(keyEvents, JSON.stringify(events));
  return new Response(JSON.stringify(resp), { headers: cors({ 'Content-Type': 'application/json' }) });
}

// ---------------- VIBE TRADER (Aster + OpenAI) ----------------
type VibeConfig = {
  status: 'running' | 'stopped';
  universe: string[];
  maxRiskPerTradeUsd: number;
  maxDailyLossUsd: number;
  maxExposureUsd: number;
  leverageCap: number;
  marginMode: 'cross' | 'isolated';
  model: string;
};

type VibeRuntime = {
  lastTickAt?: number;
  sessionLossUsd?: number;
  lastError?: string | null;
  lastProvider?: 'qwen' | 'openai' | undefined;
  lastModel?: string | undefined;
  lastOrderAt?: number | undefined;
  lastSignal?: 'LONG' | 'SHORT' | 'FLAT' | undefined;
};

const DEFAULT_VIBE_CONFIG: VibeConfig = {
  status: 'running',
  universe: ['BTCUSDT', 'ETHUSDT'],
  maxRiskPerTradeUsd: 50,
  maxDailyLossUsd: 200,
  maxExposureUsd: 2000,
  leverageCap: 5,
  marginMode: 'cross',
  model: 'gpt-4o-mini'
};

async function kvGetJson<T>(env: Env, url: URL, key: string, fallback: T): Promise<T> {
  const fullKey = new URL(key, url).toString();
  const data = await env.MEAP_KV.get(fullKey, { type: 'json' });
  return (data as T) ?? fallback;
}

async function kvPutJson(env: Env, url: URL, key: string, value: any): Promise<void> {
  const fullKey = new URL(key, url).toString();
  await env.MEAP_KV.put(fullKey, JSON.stringify(value));
}

async function appendLog(env: Env, url: URL, entry: any) {
  const logs: any[] = await kvGetJson(env, url, '/vibe_logs.json', []);
  logs.push({ at: Date.now(), ...entry });
  if (logs.length > 500) logs.splice(0, logs.length - 500);
  await kvPutJson(env, url, '/vibe_logs.json', logs);
}

async function handleVibeStatus(req: Request, env: Env) {
  const url = new URL(req.url);
  const cfg = await kvGetJson<VibeConfig>(env, url, '/vibe_config.json', DEFAULT_VIBE_CONFIG);
  const rt = await kvGetJson<VibeRuntime>(env, url, '/vibe_runtime.json', {} as VibeRuntime);
  return new Response(JSON.stringify({ ok: true, config: cfg, runtime: rt }, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleVibeRun(req: Request, env: Env) {
  const url = new URL(req.url);
  // lock behind admin secret
  const admin = env.ADMIN_KEY;
  const key = req.headers.get('x-admin-key') || '';
  if (!admin || key !== admin) {
    return new Response(JSON.stringify({ ok: false, error: 'forbidden' }), { status: 403, headers: cors({ 'Content-Type': 'application/json' }) });
  }
  const body: any = await req.json().catch(() => ({}));
  const cfg = await kvGetJson<VibeConfig>(env, url, '/vibe_config.json', DEFAULT_VIBE_CONFIG);
  const next: VibeConfig = {
    ...cfg,
    status: 'running',
    universe: Array.isArray(body?.universe) && body.universe.length ? body.universe : cfg.universe,
    maxRiskPerTradeUsd: Number(body?.maxRiskPerTradeUsd ?? cfg.maxRiskPerTradeUsd),
    maxDailyLossUsd: Number(body?.maxDailyLossUsd ?? cfg.maxDailyLossUsd),
    maxExposureUsd: Number(body?.maxExposureUsd ?? cfg.maxExposureUsd),
    leverageCap: Number(body?.leverageCap ?? cfg.leverageCap),
    marginMode: body?.marginMode === 'isolated' ? 'isolated' : 'cross',
    model: typeof body?.model === 'string' && body.model ? body.model : cfg.model
  };
  await kvPutJson(env, url, '/vibe_config.json', next);
  await appendLog(env, url, { type: 'vibe_status', status: 'running' });
  return new Response(JSON.stringify({ ok: true, config: next }), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleVibeStop(req: Request, env: Env) {
  const url = new URL(req.url);
  // lock behind admin secret
  const admin = env.ADMIN_KEY;
  const key = req.headers.get('x-admin-key') || '';
  if (!admin || key !== admin) {
    return new Response(JSON.stringify({ ok: false, error: 'forbidden' }), { status: 403, headers: cors({ 'Content-Type': 'application/json' }) });
  }
  const cfg = await kvGetJson<VibeConfig>(env, url, '/vibe_config.json', DEFAULT_VIBE_CONFIG);
  const next: VibeConfig = { ...cfg, status: 'stopped' };
  await kvPutJson(env, url, '/vibe_config.json', next);
  await appendLog(env, url, { type: 'vibe_status', status: 'stopped' });
  return new Response(JSON.stringify({ ok: true, config: next }), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function callLLMDecider(env: Env, state: any, cfg: VibeConfig): Promise<any> {
  // Prefer Qwen if available; otherwise use OpenAI
  const useQwen = !!env.QWEN_API_KEY;
  const apiKey = useQwen ? env.QWEN_API_KEY : env.OPENAI_API_KEY;
  if (!apiKey) throw new Error('LLM_API_KEY missing');
  const baseUrl = useQwen ? (env.QWEN_BASE_URL || 'https://dashscope.aliyuncs.com/compatible-mode/v1') : 'https://api.openai.com/v1';
  // Default model per provider
  let model = cfg.model || (useQwen ? 'qwen2.5-32b-instruct' : 'gpt-4o-mini');
  if (useQwen && /^gpt/i.test(model)) model = 'qwen2.5-32b-instruct';
  const sys = `You are a futures trading decider. Output strict JSON with keys: action(one of LONG,SHORT,FLAT), symbol, size_usd(number), notes(string). Respect risk limits: maxRiskPerTradeUsd=${cfg.maxRiskPerTradeUsd}, maxExposureUsd=${cfg.maxExposureUsd}. Allowed symbols: ${cfg.universe.join(', ')}`;
  const user = { role: 'user', content: `State: ${JSON.stringify(state).slice(0, 5000)}` } as const;
  const r = await fetch(`${baseUrl}/chat/completions`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${apiKey}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      model,
      response_format: { type: 'json_object' },
      messages: [ { role: 'system', content: sys }, user ]
    })
  });
  if (!r.ok) throw new Error(`llm_http_${r.status}`);
  const data: any = await r.json();
  const content = data?.choices?.[0]?.message?.content || '{}';
  let parsed: any = {};
  try { parsed = JSON.parse(content); } catch {}
  return { parsed, meta: { provider: useQwen ? 'qwen' : 'openai', model, sys, stateSummary: { equityUsd: state?.balances?.equityUsd, positionsCount: state?.positions?.length ?? 0 } } };
}

async function vibeTick(env: Env, url: URL, ignoreStatus: boolean = false) {
  const cfg = await kvGetJson<VibeConfig>(env, url, '/vibe_config.json', DEFAULT_VIBE_CONFIG);
  const rt = await kvGetJson<VibeRuntime>(env, url, '/vibe_runtime.json', {} as VibeRuntime);
  const eventsKey = new URL('/__events.json', url).toString();
  const events = ((await env.MEAP_KV.get(eventsKey, { type: 'json' })) as any[]) || [];
  if (cfg.status !== 'running' && !ignoreStatus) return { skipped: 'stopped' };

  // Pull account available balance for equity sampling
  let availableBalance = 0;
  try {
    const acct = await asterFapiGetJson(env, '/fapi/v2/account');
    availableBalance = Number(acct?.availableBalance || '0');
  } catch {}
  const state = { balances: { equityUsd: availableBalance }, positions: [], universe: cfg.universe };

  try {
    // Compute signals for universe
    const syms = cfg.universe;
    let selectedAction: 'LONG' | 'SHORT' | 'FLAT' = 'FLAT';
    let selectedSymbol = syms[0];
    for (const sym of syms) {
      const kl = await asterFapiGetPublicJson(env, `/fapi/v1/klines?symbol=${sym}&interval=1m&limit=60`);
      const closes: number[] = Array.isArray(kl) ? kl.map((c: any) => Number(c[4])) : [];
      if (closes.length >= 30) {
        const fast = sma(closes, 9).at(-1) as number;
        const slow = sma(closes, 21).at(-1) as number;
        const rr = rsi(closes, 14).at(-1) as number;
        if (!Number.isNaN(fast) && !Number.isNaN(slow) && !Number.isNaN(rr)) {
          if (fast > slow && rr < 60) { selectedAction = 'LONG'; selectedSymbol = sym; break; }
          if (fast < slow && rr > 40) { selectedAction = 'SHORT'; selectedSymbol = sym; break; }
        }
      }
    }
    let meta = { provider: 'signals', model: 'ma9/ma21+rsi14', sys: 'signals' } as any;
    if (selectedAction === 'FLAT') {
      const llm = await callLLMDecider(env, state, cfg);
      meta = llm.meta;
      await appendLog(env, url, { type: 'vibe_prompt', provider: meta.provider, model: meta.model, sys: meta.sys, state: { equityUsd: state.balances.equityUsd, positionsCount: state.positions.length } });
      const sym = typeof llm.parsed?.symbol === 'string' && cfg.universe.includes(llm.parsed.symbol) ? llm.parsed.symbol : cfg.universe[0];
      const act = ['LONG','SHORT','FLAT'].includes(llm.parsed?.action) ? llm.parsed.action : 'FLAT';
      selectedAction = act as any;
      selectedSymbol = sym;
    }
    // Size clamp
    const sizeUsd = Math.min(cfg.maxRiskPerTradeUsd, Math.max(0, Math.floor(availableBalance * 0.05)));

    // Log decision
    const log = { type: 'vibe_decision', action: selectedAction, symbol: selectedSymbol, sizeUsd, notes: '' };
    await appendLog(env, url, log);
    events.push({ type: 'vibe_tick', at: Date.now(), ...log });
    await env.MEAP_KV.put(eventsKey, JSON.stringify(events));

    // Execute tiny live order if allowed and balances exist
    try {
      if ((selectedAction === 'LONG' || selectedAction === 'SHORT') && env.ASTER_API_KEY && env.ASTER_API_SECRET) {
        const notional = sizeUsd;
        const now = Date.now();
        const coolOk = !rt.lastOrderAt || now - rt.lastOrderAt > 10 * 60 * 1000;
        if (coolOk && notional >= 5) {
          const priceJ = await asterFapiGetPublicJson(env, `/fapi/v1/ticker/price?symbol=${selectedSymbol}`);
          const price = Number(priceJ?.price || 0);
          if (price > 0) {
            const qty = Math.max(0.0001, Number((notional / price).toFixed(4)));
            const side = selectedAction === 'LONG' ? 'BUY' : 'SELL';
            const r = await asterFapiSignedPost(env, '/fapi/v1/order', { symbol: selectedSymbol, side, type: 'MARKET', quantity: qty });
            const txt = await r.text();
            const body = (() => { try { return JSON.parse(txt); } catch { return { raw: txt }; } })();
            await appendLog(env, url, { type: 'vibe_order', status: r.status, ok: r.ok, symbol: selectedSymbol, side, qty, notional, body });
            if (r.ok) {
              rt.lastOrderAt = now;
              rt.lastSignal = selectedAction;
            }
          }
        }
      }
    } catch (e: any) {
      await appendLog(env, url, { type: 'vibe_order_error', error: String(e?.message || e) });
    }

    // equity history sample
    try {
      const equity: any[] = await kvGetJson(env, url, '/vibe_equity.json', []);
      equity.push({ at: Date.now(), equityUsd: state.balances.equityUsd });
      if (equity.length > 1440) equity.splice(0, equity.length - 1440);
      await kvPutJson(env, url, '/vibe_equity.json', equity);
    } catch {}

    // positions snapshot (read-only for now; will be replaced with Aster response)
    try {
      await kvPutJson(env, url, '/vibe_positions.json', { positions: state.positions });
    } catch {}

    const nextRt: VibeRuntime = { ...rt, lastTickAt: Date.now(), lastError: null, lastProvider: meta.provider, lastModel: meta.model };
    await kvPutJson(env, url, '/vibe_runtime.json', nextRt);
    return { ok: true, decision: log, meta };
  } catch (e: any) {
    const err = String(e?.message || e);
    await appendLog(env, url, { type: 'vibe_error', error: err });
    const nextRt: VibeRuntime = { ...rt, lastTickAt: Date.now(), lastError: err };
    await kvPutJson(env, url, '/vibe_runtime.json', nextRt);
    return { ok: false, error: err };
  }
}

async function handleVibeTick(req: Request, env: Env) {
  const url = new URL(req.url);
  const res = await vibeTick(env, url);
  return new Response(JSON.stringify(res, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleVibeLogs(req: Request, env: Env) {
  const url = new URL(req.url);
  const logs = await kvGetJson<any[]>(env, url, '/vibe_logs.json', []);
  return new Response(JSON.stringify({ logs: logs.slice(-200).reverse() }, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleVibePositions(req: Request, env: Env) {
  const url = new URL(req.url);
  // Live fetch from Aster (Binance-style fapi). No fallback.
  try {
    const base = (env.ASTER_API_BASE || '').trim();
    const apiKey = env.ASTER_API_KEY;
    const apiSecret = env.ASTER_API_SECRET;
    if (!base || !apiKey || !apiSecret) {
      return new Response(JSON.stringify({ ok: false, error: 'ASTER_API_BASE/API_KEY/API_SECRET missing' }, null, 2), { status: 400, headers: cors({ 'Content-Type': 'application/json' }) });
    }
    const ts = Date.now();
    const recv = 5000;
    const query = `timestamp=${ts}&recvWindow=${recv}`;
    const sig = await hmacHex('SHA-256', apiSecret, query);
    const r = await fetch(`${base}/fapi/v2/positionRisk?${query}&signature=${sig}`, { headers: { 'X-MBX-APIKEY': apiKey } });
    const txt = await r.text();
    const body = (() => { try { return JSON.parse(txt); } catch { return { raw: txt }; } })();
    return new Response(JSON.stringify({ status: r.status, ok: r.ok, body }, null, 2), { status: r.status, headers: cors({ 'Content-Type': 'application/json' }) });
  } catch (e: any) {
    return new Response(JSON.stringify({ ok: false, error: String(e?.message || e) }, null, 2), { status: 500, headers: cors({ 'Content-Type': 'application/json' }) });
  }
}

async function handleVibeBalances(req: Request, env: Env) {
  const url = new URL(req.url);
  // Live fetch from Aster (Binance-style fapi). No fallback.
  try {
    const base = (env.ASTER_API_BASE || '').trim();
    const apiKey = env.ASTER_API_KEY;
    const apiSecret = env.ASTER_API_SECRET;
    if (!base || !apiKey || !apiSecret) {
      return new Response(JSON.stringify({ ok: false, error: 'ASTER_API_BASE/API_KEY/API_SECRET missing' }, null, 2), { status: 400, headers: cors({ 'Content-Type': 'application/json' }) });
    }
    const ts = Date.now();
    const recv = 5000;
    const query = `timestamp=${ts}&recvWindow=${recv}`;
    const sig = await hmacHex('SHA-256', apiSecret, query);
    const r = await fetch(`${base}/fapi/v2/account?${query}&signature=${sig}`, { headers: { 'X-MBX-APIKEY': apiKey } });
    const txt = await r.text();
    const body = (() => { try { return JSON.parse(txt); } catch { return { raw: txt }; } })();
    return new Response(JSON.stringify({ status: r.status, ok: r.ok, body }, null, 2), { status: r.status, headers: cors({ 'Content-Type': 'application/json' }) });
  } catch (e: any) {
    return new Response(JSON.stringify({ ok: false, error: String(e?.message || e) }, null, 2), { status: 500, headers: cors({ 'Content-Type': 'application/json' }) });
  }
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    if (req.method === 'OPTIONS') return new Response(null, { status: 204, headers: cors() });
    if (url.pathname === '/api/debug') return handleDebug(req, env);
    if (url.pathname === '/api/events' || url.pathname === '/api/feed' || url.pathname === '/feed') return handleEvents(req, env);
    if (url.pathname === '/api/agents' && req.method === 'GET') return handleAgentsGet(req, env);
    if (url.pathname === '/api/agents' && req.method === 'POST') return handleAgentsPost(req, env);
    if (url.pathname === '/api/messages' && req.method === 'POST') return handleMessage(req, env);
    // Vibe Trader routes
    if (url.pathname === '/api/vibe/status' && req.method === 'GET') return handleVibeStatus(req, env);
    if (url.pathname === '/api/vibe/run' && req.method === 'POST') return handleVibeRun(req, env);
    if (url.pathname === '/api/vibe/stop' && req.method === 'POST') return handleVibeStop(req, env);
    if (url.pathname === '/api/vibe/tick' && req.method === 'POST') return handleVibeTick(req, env);
    if (url.pathname === '/api/vibe/logs' && req.method === 'GET') return handleVibeLogs(req, env);
    if (url.pathname === '/api/vibe/positions' && req.method === 'GET') return handleVibePositions(req, env);
    if (url.pathname === '/api/vibe/balances' && req.method === 'GET') return handleVibeBalances(req, env);
    if (url.pathname === '/api/vibe/equity' && req.method === 'GET') {
      const eq = (await env.MEAP_KV.get(new URL('/vibe_equity.json', url).toString(), { type: 'json' })) || [];
      return new Response(JSON.stringify({ equity: Array.isArray(eq) ? eq.slice(-1000) : [] }, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
    }
    // Aster diagnostics (no secrets)
    if (url.pathname === '/api/vibe/aster-test' && req.method === 'GET') {
      const results: any = { base: env.ASTER_API_BASE ? true : false };
      try {
        const r1 = await asterRequest(env, 'GET', '/v1/futures/account');
        results.account = { status: r1.status, ok: r1.ok };
      } catch (e: any) { results.account = { error: String(e?.message || e) }; }
      try {
        const r2 = await asterRequest(env, 'GET', '/v1/futures/positions');
        results.positions = { status: r2.status, ok: r2.ok };
      } catch (e: any) { results.positions = { error: String(e?.message || e) }; }
      return new Response(JSON.stringify(results, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
    }
    if (url.pathname === '/api/vibe/aster-debug' && req.method === 'GET') {
      const hasPriv = !!env.ASTER_PRIVATE_KEY;
      const addr = hasPriv ? evmAddressFromPrivateKey(env.ASTER_PRIVATE_KEY as string) : null;
      const hasApiKey = !!env.ASTER_API_KEY;
      const hasApiSecret = !!env.ASTER_API_SECRET;
      return new Response(JSON.stringify({ base: env.ASTER_API_BASE || null, evm: { enabled: hasPriv, address: addr }, apiKey: { enabled: hasApiKey && hasApiSecret } }, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
    }
    // Place tiny market order (notional-based): /api/vibe/order/market?symbol=BTCUSDT&notional=10
    // Disabled public order endpoint to prevent external influence
    // Set leverage: /api/vibe/leverage?symbol=BTCUSDT&leverage=5
    // Disabled public leverage endpoint
    // Cancel all: /api/vibe/cancelAll?symbol=BTCUSDT
    // Disabled public cancel endpoint
    // /api/agents/:id/inbox
    const inboxMatch = url.pathname.match(/^\/api\/agents\/([^/]+)\/inbox$/);
    if (inboxMatch && req.method === 'GET') {
      const agentId = inboxMatch[1];
      const keyInbox = new URL(`/inbox_${agentId}.json`, url).toString();
      const messages = (await env.MEAP_KV.get(keyInbox, { type: 'json' })) || [];
      return new Response(JSON.stringify({ messages }), { headers: cors({ 'Content-Type': 'application/json' }) });
    }
    return new Response('Not Found', { status: 404, headers: cors() });
  },
  async scheduled(controller: ScheduledController, env: Env, ctx: ExecutionContext) {
    const url = new URL('https://api.meap.fun'); // base for KV key scoping; actual host not used
    await vibeTick(env, url, true);
  }
};


