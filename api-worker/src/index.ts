// crypto libs will be loaded lazily when wiring Aster signing

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

  // TODO: Pull Aster account/positions. For now, minimal stub state.
  const state = { balances: { equityUsd: 10000 }, positions: [], universe: cfg.universe };

  try {
    const { parsed, meta } = await callLLMDecider(env, state, cfg);
    // log prompt/summary with provider
    await appendLog(env, url, { type: 'vibe_prompt', provider: meta.provider, model: meta.model, sys: meta.sys, state: { equityUsd: state.balances.equityUsd, positionsCount: state.positions.length } });
    // Guardrails
    const symbol = typeof parsed?.symbol === 'string' && cfg.universe.includes(parsed.symbol) ? parsed.symbol : cfg.universe[0];
    const action = ['LONG','SHORT','FLAT'].includes(parsed?.action) ? parsed.action : 'FLAT';
    const sizeUsdRaw = Number(parsed?.size_usd || 0);
    const sizeUsd = Math.max(0, Math.min(sizeUsdRaw, cfg.maxExposureUsd));

    // Placeholder: no real order until Aster client is wired. Log intent only.
    const log = { type: 'vibe_decision', action, symbol, sizeUsd, notes: parsed?.notes || '' };
    await appendLog(env, url, log);
    events.push({ type: 'vibe_tick', at: Date.now(), ...log });
    await env.MEAP_KV.put(eventsKey, JSON.stringify(events));

    // balance history sample
    try {
      const balances: any[] = await kvGetJson(env, url, '/vibe_balances.json', []);
      balances.push({ at: Date.now(), equityUsd: state.balances.equityUsd });
      if (balances.length > 1440) balances.splice(0, balances.length - 1440); // ~1 day at 1-min ticks
      await kvPutJson(env, url, '/vibe_balances.json', balances);
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
  const pos = await kvGetJson<any>(env, url, '/vibe_positions.json', { positions: [] });
  return new Response(JSON.stringify(pos, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
}

async function handleVibeBalances(req: Request, env: Env) {
  const url = new URL(req.url);
  const arr = await kvGetJson<any[]>(env, url, '/vibe_balances.json', []);
  return new Response(JSON.stringify({ balances: arr.slice(-1000) }, null, 2), { headers: cors({ 'Content-Type': 'application/json' }) });
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


