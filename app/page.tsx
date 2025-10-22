"use client";

import { useEffect, useMemo, useState } from 'react';
// Hardcode API base so the panel works without env vars. Replace if your URL differs.
const API_BASE = 'https://api.meap.fun';

export default function Page() {
  return (
    <main className="screen">
      <div className="shell" style={{ gridTemplateColumns: '1fr' }}>
        <section className="panel" style={{ position: 'relative' }}>
          <Hero />
          <MainDashboard />
        </section>
      </div>
    </main>
  );
}

function useVibeData() {
  const [status, setStatus] = useState<any>(null);
  const [logs, setLogs] = useState<any[]>([]);
  const [equity, setEquity] = useState<any[]>([]);
  const [account, setAccount] = useState<any>(null);
  const [positions, setPositions] = useState<any[]>([]);

  async function refresh() {
    try {
      const [s, l, eq, bal, pos] = await Promise.all([
        fetch(`${API_BASE}/api/vibe/status?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>null),
        fetch(`${API_BASE}/api/vibe/logs?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>({ logs: [] })),
        fetch(`${API_BASE}/api/vibe/equity?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>({ equity: [] })),
        fetch(`${API_BASE}/api/vibe/balances?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>null),
        fetch(`${API_BASE}/api/vibe/positions?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>null)
      ]);
      if (s) setStatus(s);
      if (Array.isArray(l?.logs)) setLogs(l.logs);
      if (Array.isArray(eq?.equity)) setEquity(eq.equity);
      if (bal && bal.status === 200) setAccount(bal.body);
      if (pos && pos.status === 200 && Array.isArray(pos.body)) setPositions(pos.body);
    } catch {}
  }

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 5000);
    return () => clearInterval(id);
  }, []);

  return { status, logs, equity, account, positions };
}

function Hero() {
  const { status, account } = useVibeData();
  const available = account ? Number(account.availableBalance || 0) : 0;
  const provider = status?.runtime?.lastProvider || '—';
  const model = status?.runtime?.lastModel || status?.config?.model || '—';
  const universe = (status?.config?.universe || []).join(', ');
  const lastTick = status?.runtime?.lastTickAt ? new Date(status.runtime.lastTickAt).toLocaleTimeString() : '—';

  return (
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
      <div>
        <div style={{ fontSize: 22, fontWeight: 800 }}>Total Account Value</div>
        <div style={{ color: 'var(--muted)', marginTop: 4 }}>Universe: {universe}</div>
      </div>
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
        <span className="pill">LLM: {provider}/{model}</span>
        <span className="pill">Available: ${available.toFixed(2)}</span>
        <span className="pill">Last tick: {lastTick}</span>
      </div>
    </div>
  );
}

function MainDashboard() {
  const { status, logs, equity, positions, account } = useVibeData();
  const [tab, setTab] = useState<'trades' | 'positions' | 'logs'>('trades');
  const eqPoints = Array.isArray(equity) ? equity.slice(-200) : [];
  const trades = useMemo(() => (logs || []).filter((e: any) => e.type === 'vibe_order').slice(0, 20), [logs]);
  const err = status?.runtime?.lastError || null;
  const posOpen = positions?.filter((p:any)=>Number(p.positionAmt) !== 0).length || 0;
  const available = account ? Number(account.availableBalance || 0) : 0;

  return (
    <div>
      <div style={{ height: 280, marginBottom: 10, borderRadius: 12, border: '1px solid var(--border)', background: '#fff', position: 'relative', padding: 8 }}>
        <div style={{ position: 'absolute', top: 10, left: 12, fontWeight: 700, fontSize: 12, color: 'var(--muted)' }}>EQUITY (USD)</div>
        <svg width="100%" height="100%" viewBox="0 0 100 20" preserveAspectRatio="none">
          {eqPoints.length >= 2 && (() => {
            const vals = eqPoints.map((p: any) => Number(p.equityUsd) || 0);
            const min = Math.min(...vals);
            const max = Math.max(...vals);
            const span = max - min || 1;
            const pts = vals.map((v, i) => {
              const x = (i / (vals.length - 1)) * 100;
              const y = 20 - ((v - min) / span) * 20;
              return `${x},${y}`;
            }).join(' ');
            return <polyline fill="none" stroke="var(--accent)" strokeWidth="1.6" points={pts} />;
          })()}
        </svg>
      </div>

      <div style={{ display: 'flex', gap: 10, marginBottom: 12, flexWrap: 'wrap' }}>
        <div className="pill">Open positions: {posOpen}</div>
        <div className="pill">Available: ${available.toFixed(2)}</div>
        <div className="pill">Status: {status?.config?.status === 'running' ? 'Running' : 'Stopped'}</div>
        {err && <div className="left-pill">{String(err).slice(0,120)}</div>}
      </div>

      <div className="tabs" style={{ marginBottom: 10 }}>
        <div className={`tab ${tab==='trades'?'active':''}`} onClick={() => setTab('trades')}>Completed Trades</div>
        <div className={`tab ${tab==='positions'?'active':''}`} onClick={() => setTab('positions')}>Positions</div>
        <div className={`tab ${tab==='logs'?'active':''}`} onClick={() => setTab('logs')}>Logs</div>
      </div>

      {tab === 'trades' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {trades.length === 0 && (<div className="stream-item"><span className="badge">Trade</span><span style={{ color: 'var(--muted)' }}>No completed trades yet</span></div>)}
          {trades.map((t: any, i: number) => (
            <div key={i} className="stream-item">
              <span className="badge">{t.symbol || '—'}</span>
              <span style={{ color: 'var(--muted)' }}>{t.side || '—'} {t.qty ?? '—'} {t.notional ? `($${Number(t.notional).toFixed(2)})` : ''}</span>
            </div>
          ))}
        </div>
      )}

      {tab === 'positions' && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8 }}>
          {positions.slice(0, 9).map((p: any, i: number) => (
            <div key={i} className="stream-item">
              <span className="badge">{p.symbol}</span>
              <span style={{ color: 'var(--muted)' }}>{p.positionAmt} @ {p.entryPrice} | PnL: {p.unRealizedProfit}</span>
            </div>
          ))}
          {positions.length === 0 && <div className="stream-item"><span className="badge">Pos</span><span style={{ color: 'var(--muted)' }}>None</span></div>}
        </div>
      )}

      {tab === 'logs' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {(logs || []).slice(0, 20).map((e: any, i: number) => (
            <div className="stream-item" key={i}>
              <span className="badge">{String(e.type || 'Log').replace('vibe_', '')}</span>
              <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--muted)' }}>
                {e.type === 'vibe_order' ? `${e.symbol || ''} ${e.side || ''} ${e.qty || ''}` : e.type === 'vibe_decision' ? `${e.action || ''} ${e.symbol || ''} $${e.sizeUsd ?? e.size_usd ?? ''}` : e.error || e.type}
              </span>
            </div>
          ))}
          {(!logs || logs.length === 0) && (
            <div className="stream-item"><span className="badge">Log</span><span style={{ color: 'var(--muted)' }}>Waiting for first tick…</span></div>
          )}
        </div>
      )}
    </div>
  );
}

// Legacy components removed for Aster trading revamp
