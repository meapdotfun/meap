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
  const [balances, setBalances] = useState<any[]>([]);

  async function refresh() {
    try {
      const [s, l] = await Promise.all([
        fetch(`${API_BASE}/api/vibe/status?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>null),
        fetch(`${API_BASE}/api/vibe/logs?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>({ logs: [] })),
        fetch(`${API_BASE}/api/vibe/balances?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(()=>({ balances: [] }))
      ]);
      if (s) setStatus(s);
      if (Array.isArray(l?.logs)) setLogs(l.logs);
      if (Array.isArray((s as any)?.balances)) setBalances((s as any).balances);
      if (!Array.isArray((s as any)?.balances) && Array.isArray((arguments as any)[0]?.balances)) setBalances((arguments as any)[0].balances);
      if (Array.isArray((arguments as any)[2]?.balances)) setBalances((arguments as any)[2].balances);
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
  const bal = Array.isArray(balances) ? balances.slice(-100) : [];

  return (
    <div style={{ marginBottom: 14 }}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8, marginBottom: 8 }}>
        <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
          <span className="badge">Vibe</span>
          <span style={{ fontWeight: 700 }}>{status?.config?.status === 'running' ? 'Running' : 'Stopped'}</span>
          <span style={{ opacity: 0.7 }}>LLM: {activeProvider}/{activeModel}</span>
          <span style={{ opacity: 0.7 }}>Universe: {(status?.config?.universe || []).join(', ')}</span>
          <span style={{ opacity: 0.7 }}>Last tick: {lastTick}</span>
          {err && <span className="left-pill" style={{ background: '#3b1a1a', color: '#ffd0d0' }}>{String(err).slice(0,120)}</span>}
        </div>
        {/* No manual actions; fully autonomous */}
      </div>
      {/* Balance sparkline */}
      <div style={{ height: 48, marginBottom: 10 }}>
        <svg width="100%" height="100%" viewBox="0 0 100 20" preserveAspectRatio="none">
          {bal.length >= 2 && (() => {
            const vals = bal.map((p: any) => Number(p.equityUsd) || 0);
            const min = Math.min(...vals);
            const max = Math.max(...vals);
            const span = max - min || 1;
            const pts = vals.map((v, i) => {
              const x = (i / (vals.length - 1)) * 100;
              const y = 20 - ((v - min) / span) * 20;
              return `${x},${y}`;
            }).join(' ');
            return <polyline fill="none" stroke="#ffdb01" strokeWidth="0.8" points={pts} />;
          })()}
        </svg>
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
        {logs.slice(0, 10).map((e, i) => (
          <div className="stream-item" key={i}>
            <span className="badge">Log</span>
            <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
              {e.type === 'vibe_decision' ? `${e.action || ''} ${e.symbol || ''} $${e.sizeUsd ?? e.size_usd ?? ''}` : e.type}
            </span>
          </div>
        ))}
        {logs.length === 0 && (
          <div className="stream-item"><span className="badge">Log</span><span>Waiting for first tick…</span></div>
        )}
      </div>
    </div>
  );
}

// Legacy components removed for Aster trading revamp
