"use client";

import { useEffect, useState } from 'react';
// Hardcode API base so the panel works without env vars. Replace if your URL differs.
const API_BASE = 'https://api.meap.fun';

export default function Page() {
  return (
    <main className="screen">
      <div className="shell">
        <section className="panel" style={{ position: 'relative' }}>
          <VibePanel />
        </section>
      </div>
    </main>
  );
}

function VibePanel() {
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
    const id = setInterval(refresh, 4000);
    return () => clearInterval(id);
  }, []);

  const lastTick = status?.runtime?.lastTickAt ? new Date(status.runtime.lastTickAt).toLocaleTimeString() : '—';
  const activeProvider = status?.runtime?.lastProvider || '—';
  const activeModel = status?.runtime?.lastModel || status?.config?.model || '—';
  const err = status?.runtime?.lastError || null;
  const eqPoints = Array.isArray(equity) ? equity.slice(-100) : [];
  const available = account ? Number(account.availableBalance || 0) : 0;
  const posOpen = positions?.filter((p:any)=>Number(p.positionAmt) !== 0).length || 0;

  return (
    <div style={{ marginBottom: 4 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 12, flexWrap: 'wrap' }}>
        <span className="badge">Vibe</span>
        <span style={{ fontWeight: 800 }}>{status?.config?.status === 'running' ? 'Running' : 'Stopped'}</span>
        <span style={{ color: 'var(--muted)' }}>LLM: {activeProvider}/{activeModel}</span>
        <span style={{ color: 'var(--muted)' }}>Universe: {(status?.config?.universe || []).join(', ')}</span>
        <span style={{ color: 'var(--muted)' }}>Last tick: {lastTick}</span>
        {err && <span className="left-pill">{String(err).slice(0,120)}</span>}
      </div>
      <div style={{ height: 240, marginBottom: 16, borderRadius: 10, border: '1px solid var(--border)', background: '#fff' }}>
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
      <div style={{ display: 'flex', gap: 10, marginBottom: 14, flexWrap: 'wrap' }}>
        <div className="pill">Available: ${available.toFixed(2)}</div>
        <div className="pill">Open positions: {posOpen}</div>
        <div className="pill">Universe: {(status?.config?.universe || []).join(', ')}</div>
        <div className="pill">LLM: {activeProvider}/{activeModel}</div>
      </div>
      <div style={{ marginBottom: 12 }}>
        <div style={{ fontWeight: 800, marginBottom: 8 }}>Positions</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8 }}>
          {positions.slice(0, 6).map((p: any, i: number) => (
            <div key={i} className="stream-item">
              <span className="badge">{p.symbol}</span>
              <span style={{ color: 'var(--muted)' }}>{p.positionAmt} @ {p.entryPrice}</span>
            </div>
          ))}
          {positions.length === 0 && <div className="stream-item"><span className="badge">Pos</span><span style={{ color: 'var(--muted)' }}>None</span></div>}
        </div>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {logs.slice(0, 12).map((e, i) => (
          <div className="stream-item" key={i}>
            <span className="badge">Log</span>
            <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', color: 'var(--muted)' }}>
              {e.type === 'vibe_decision' ? `${e.action || ''} ${e.symbol || ''} $${e.sizeUsd ?? e.size_usd ?? ''}` : e.type}
            </span>
          </div>
        ))}
        {logs.length === 0 && (
          <div className="stream-item"><span className="badge">Log</span><span style={{ color: 'var(--muted)' }}>Waiting for first tick…</span></div>
        )}
      </div>
    </div>
  );
}

// Legacy components removed for Aster trading revamp
