"use client";

import { useEffect, useState } from 'react';
// Hardcode API base so the panel works without env vars. Replace if your URL differs.
const API_BASE = 'https://api.meap.fun';

export default function Page() {
  return (
    <div className="page">
      <header className="navbar">
        <div className="brand">
          <span className="brand-dot" />
          <span>MEAP</span>
        </div>
        <div className="nav-right">
          <span className="chip">BETA</span>
        </div>
      </header>
      <main className="container">
        <section className="card">
          <VibePanel />
        </section>
      </main>
      <style jsx global>{`
        :root {
          --bg: #f6f6f6;
          --card: #ffffff;
          --text: #1c1c1c;
          --muted: #6b6b6b;
          --accent: #ffdb01;
          --border: #e9e9e9;
        }
        html, body { height: 100%; }
        body { margin: 0; background: var(--bg); color: var(--text); font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, "Helvetica Neue", Arial, "Apple Color Emoji", "Segoe UI Emoji"; }
        * { box-sizing: border-box; }
        .page { min-height: 100vh; display: flex; flex-direction: column; }
        .navbar { height: 60px; display: flex; align-items: center; justify-content: space-between; padding: 0 20px; border-bottom: 1px solid var(--border); background: var(--card); }
        .brand { display: flex; align-items: center; gap: 10px; font-weight: 800; letter-spacing: 0.2px; }
        .brand-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--accent); display: inline-block; }
        .nav-right { display: flex; align-items: center; gap: 10px; }
        .chip { padding: 4px 10px; border-radius: 999px; background: #f0f0f0; color: #222; font-size: 12px; font-weight: 700; border: 1px solid var(--border); }
        .container { max-width: 980px; width: 100%; margin: 0 auto; padding: 18px 20px 40px; }
        .card { background: var(--card); border: 1px solid var(--border); border-radius: 14px; padding: 16px; box-shadow: 0 1px 0 rgba(0,0,0,0.03); }
        .pill { padding: 6px 10px; border-radius: 10px; background: #fafafa; border: 1px solid var(--border); color: #222; font-weight: 600; }
        .badge { padding: 4px 10px; border-radius: 999px; background: #111; color: #fff; font-weight: 800; font-size: 12px; }
        .left-pill { padding: 6px 10px; border-radius: 10px; border: 1px solid #7a2f2f; background: #fff5f5; }
        .stream-item { display: flex; align-items: center; gap: 10px; justify-content: space-between; padding: 10px 12px; border: 1px solid var(--border); border-radius: 10px; background: #fff; }
        @media (max-width: 720px) {
          .container { padding: 12px; }
          .navbar { padding: 0 12px; }
        }
      `}</style>
    </div>
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
