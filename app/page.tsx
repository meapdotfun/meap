"use client";

import { useEffect, useMemo, useRef, useState } from 'react';

const API_BASE = 'https://api.meap.fun';

export default function Page() {
  return (
    <main style={{ padding: 0 }}>
      <style jsx global>{`
        html, body { overflow: hidden; }
      `}</style>
      <div style={{ width: '100vw', height: 'calc(100vh - 64px)', display: 'flex' }}>
        <div style={{ flex: 3, minWidth: 0 }}>
          <ChartPane />
        </div>
        <div style={{ flex: 1, borderLeft: '1px solid var(--border)', background: '#fff', display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          <TradesPanel />
        </div>
      </div>
    </main>
  );
}

function useArenaData() {
  const [equity, setEquity] = useState<any[]>([]);
  const [logs, setLogs] = useState<any[]>([]);

  async function refresh() {
    try {
      const [eq, l] = await Promise.all([
        fetch(`${API_BASE}/api/vibe/equity?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(() => ({ equity: [] })),
        fetch(`${API_BASE}/api/vibe/logs?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r => r.json()).catch(() => ({ logs: [] })),
      ]);
      if (Array.isArray(eq?.equity)) setEquity(eq.equity);
      if (Array.isArray(l?.logs)) setLogs(l.logs);
    } catch {}
  }

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 5000);
    return () => clearInterval(id);
  }, []);

  return { equity, logs };
}

function ChartPane() {
  const { equity } = useArenaData();
  // Base series (last ~720 samples)
  const pointsAll = Array.isArray(equity) ? equity.slice(-720) : [];
  // Start at Oct 22, 17:30 (local year)
  const now = new Date();
  const cut = new Date(now.getFullYear(), 9, 22, 17, 30).getTime(); // month 9 = Oct
  const filtered = pointsAll.filter((p: any) => Number(p?.at || 0) >= cut);
  const points = filtered.length >= 2 ? filtered : pointsAll;
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const labelRef = useRef<HTMLDivElement | null>(null);
  const hoverRef = useRef<{ x: number | null }>({ x: null });

  useEffect(() => {
    const wrap = wrapRef.current;
    const canvas = canvasRef.current;
    if (!wrap || !canvas) return;
    const wr = wrap as HTMLDivElement;
    const cnv = canvas as HTMLCanvasElement;

    const dpr = Math.max(1, window.devicePixelRatio || 1);
    function draw() {
      const ctx = cnv.getContext('2d');
      if (!ctx) return;
      const rect = wr.getBoundingClientRect();
      const W = Math.max(200, Math.floor(rect.width));
      const H = Math.max(200, Math.floor(rect.height));
      cnv.style.width = W + 'px';
      cnv.style.height = H + 'px';
      cnv.width = Math.floor(W * dpr);
      cnv.height = Math.floor(H * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      // Clear
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(0, 0, W, H);

      // Padding for axes
      const padL = 56; // y labels
      const padR = 12;
      const padT = 18;
      const padB = 28; // x labels
      const innerW = Math.max(10, W - padL - padR);
      const innerH = Math.max(10, H - padT - padB);

      // Border line
      ctx.strokeStyle = '#f0f0f0';
      ctx.lineWidth = 1;
      ctx.strokeRect(padL, padT, innerW, innerH);

      if (!points || points.length < 2) {
        ctx.fillStyle = '#6b6b6b';
        ctx.font = '12px ui-sans-serif, system-ui';
        ctx.fillText('No data', padL + 8, padT + 20);
        return;
      }

      const vals = points.map((p: any) => Number(p?.equityUsd || 0));
      const times = points.map((p: any) => Number(p?.at || 0));
      // Use a recent window for scaling so the line isn't flat
      const windowCount = Math.min(vals.length, 240);
      const winVals = windowCount > 0 ? vals.slice(-windowCount) : vals;
      const vMinRaw = Math.min(...winVals);
      const vMaxRaw = Math.max(...winVals);
      const vSpanRaw = Math.max(1e-6, vMaxRaw - vMinRaw);

      // Dynamic y-axis: pad by ~10% of span (at least 10)
      const prePad = Math.max(vSpanRaw * 0.10, 10);
      const yMin0 = vMinRaw - prePad;
      const yMax0 = vMaxRaw + prePad;
      const span0 = Math.max(1, yMax0 - yMin0);
      const targetTicks = 6;
      let step = niceStep(span0 / targetTicks);
      const yMin = Math.floor(yMin0 / step) * step;
      const yMax = Math.ceil(yMax0 / step) * step;
      const ySpan = Math.max(step, yMax - yMin);

      const tMin = Math.min(...times);
      const tMax = Math.max(...times);
      const tSpan = Math.max(1, tMax - tMin);

      const sx = (t: number) => padL + ((t - tMin) / tSpan) * innerW;
      const sy = (v: number) => padT + (1 - (v - yMin) / ySpan) * innerH;

      // Grid + ticks
      ctx.strokeStyle = '#f3f3f3';
      ctx.lineWidth = 1;
      ctx.fillStyle = '#6b6b6b';
      ctx.font = '12px ui-sans-serif, system-ui';

      // Y ticks
      const yTicks: number[] = [];
      for (let v = yMin; v <= yMin + ySpan + 1e-9; v += step) yTicks.push(Number(v.toFixed(6)));
      yTicks.forEach(v => {
        const y = sy(v);
        ctx.beginPath();
        ctx.moveTo(padL, y);
        ctx.lineTo(padL + innerW, y);
        ctx.stroke();
        ctx.textAlign = 'right';
        ctx.textBaseline = 'middle';
        ctx.fillText(fmtUsd(v), padL - 8, y);
      });

      // X ticks
      const xTickCount = 7;
      for (let i = 0; i < xTickCount; i++) {
        const t = tMin + (i / (xTickCount - 1)) * tSpan;
        const x = sx(t);
        ctx.beginPath();
        ctx.moveTo(x, padT);
        ctx.lineTo(x, padT + innerH);
        ctx.strokeStyle = '#f7f7f7';
        ctx.stroke();
        ctx.textAlign = 'center';
        ctx.textBaseline = 'top';
        ctx.fillStyle = '#6b6b6b';
        ctx.fillText(fmtTime(t), x, padT + innerH + 6);
      }

      // Line
      ctx.beginPath();
      ctx.lineWidth = 1;
      ctx.strokeStyle = '#1c1c1c';
      points.forEach((p: any, idx: number) => {
        const x = sx(Number(p.at));
        const y = sy(Number(p.equityUsd));
        if (idx === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
      });
      ctx.stroke();

      // Pulse + label at end
      const last = points[points.length - 1];
      if (last) {
        const lx = sx(Number(last.at));
        const ly = sy(Number(last.equityUsd));
        const t = performance.now();
        const pulse = 4 + 2 * (0.5 + 0.5 * Math.sin(t / 500));
        ctx.beginPath();
        ctx.arc(lx, ly, pulse + 2, 0, Math.PI * 2);
        ctx.strokeStyle = 'rgba(28,28,28,0.20)';
        ctx.lineWidth = 1;
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(lx, ly, 2.2, 0, Math.PI * 2);
        ctx.fillStyle = '#1c1c1c';
        ctx.fill();

        const label = labelRef.current;
        if (label) {
          // Default to last point label
          let px = lx, py = ly, val = last && typeof last.equityUsd === 'number' ? fmtUsd(Number(last.equityUsd)) : '', ts = fmtTime(Number(last.at));
          // If hovering, snap to nearest time along x and draw vertical dashed line
          if (hoverRef.current.x !== null) {
            const hx = hoverRef.current.x;
            // find nearest point by x
            let bestIdx = 0, bestDist = 1e9;
            for (let i = 0; i < points.length; i++) {
              const x = sx(Number(points[i].at));
              const d = Math.abs(x - hx);
              if (d < bestDist) { bestDist = d; bestIdx = i; }
            }
            const p = points[bestIdx];
            px = sx(Number(p.at));
            py = sy(Number(p.equityUsd));
            val = fmtUsd(Number(p.equityUsd));
            ts = fmtTime(Number(p.at));
            // vertical dashed line
            ctx.save();
            ctx.setLineDash([2, 2]);
            ctx.strokeStyle = '#d4d4d4';
            ctx.lineWidth = 0.8;
            ctx.beginPath();
            ctx.moveTo(px, padT);
            ctx.lineTo(px, padT + innerH);
            ctx.stroke();
            ctx.restore();
          }
          // Anchor the label centered on the dashed guide line
          label.style.left = `${px}px`;
          label.style.top = `${py}px`;
          label.style.transform = 'translate(-50%, -130%)';
          const valEl = label.querySelector('[data-val]') as HTMLElement | null;
          const timeEl = label.querySelector('[data-time]') as HTMLElement | null;
          if (valEl) valEl.textContent = val;
          if (timeEl) timeEl.textContent = ts;
          label.style.display = 'inline-flex';
        }
      }
    }

    const ResizeObserverCtor = (window as any).ResizeObserver;
    const ro = ResizeObserverCtor ? new ResizeObserverCtor(() => draw()) : null;
    if (ro) ro.observe(wr);

    // Hover handlers
    function onMove(e: MouseEvent) {
      const rect = wr.getBoundingClientRect();
      hoverRef.current.x = e.clientX - rect.left;
    }
    function onLeave() { hoverRef.current.x = null; }
    wr.addEventListener('mousemove', onMove);
    wr.addEventListener('mouseleave', onLeave);
    let raf = 0;
    const loop = () => { draw(); raf = requestAnimationFrame(loop); };
    loop();
    return () => { try { ro && ro.disconnect(); } catch {} try { cancelAnimationFrame(raf); } catch {} wr.removeEventListener('mousemove', onMove); wr.removeEventListener('mouseleave', onLeave); };
  }, [points]);

  return (
    <div ref={wrapRef} style={{ width: '100%', height: '100%', background: '#fff', position: 'relative' }}>
      <canvas ref={canvasRef} />
      <div ref={labelRef} style={{ position: 'absolute', zIndex: 3, pointerEvents: 'none', display: 'inline-flex', alignItems: 'center', gap: 8, padding: '4px 8px', border: '1px solid var(--border)', borderRadius: 8, background: '#fff', boxShadow: '0 2px 8px rgba(0,0,0,.06)', fontWeight: 800, fontSize: 12 }}>
        <img src="/meaptrans.png" alt="" style={{ width: 14, height: 14, opacity: .85 }} />
        <span data-val>$0</span>
        <span data-time style={{ fontWeight: 600, color: '#6b6b6b', fontSize: 11 }}></span>
      </div>
    </div>
  );
}

function TradesPanel() {
  const [trades, setTrades] = useState<any[]>([]);

  useEffect(() => {
    async function load() {
      try {
        const r = await fetch(`${API_BASE}/api/vibe/trades?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' });
        const j = await r.json().catch(()=>({ trades: [] }));
        if (Array.isArray(j?.trades)) setTrades([...j.trades].reverse());
        // fallback to logs if no trades yet
        if ((!j?.trades || j.trades.length === 0)) {
          const l = await fetch(`${API_BASE}/api/vibe/logs?t=${Date.now()}`, { cache: 'no-store', mode: 'cors' }).then(r=>r.json()).catch(()=>({ logs: [] }));
          const pseudo = (l?.logs||[]).filter((e:any)=>e.type==='vibe_order').slice(-50).map((e:any)=>({
            symbol: e.symbol, side: e.side==='BUY'?'LONG':'SHORT', qty: e.qty, entryPrice: e.price, notionalEntry: e.notional, openedAt: e.at, reason: e.reason, provider: e.provider, model: e.model
          })).reverse();
          // ensure newest first (highest openedAt first)
          setTrades(pseudo.sort((a:any,b:any)=>Number(b.openedAt||0)-Number(a.openedAt||0)));
        }
      } catch {}
    }
    load();
    const id = setInterval(load, 7000);
    return () => clearInterval(id);
  }, []);

  return (
    <>
      <div style={{ height: 44, display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '0 12px', borderBottom: '1px solid var(--border)', fontWeight: 800 }}>Completed Trades</div>
      <div style={{ flex: 1, overflowY: 'auto', overflowX: 'hidden', padding: 12, display: 'flex', flexDirection: 'column', gap: 10 }}>
        {trades.length === 0 && (
          <div style={{ border: '1px solid var(--border)', borderRadius: 10, padding: '12px' }}>
            <div style={{ fontWeight: 800, marginBottom: 6 }}>No completed trades yet</div>
            <div style={{ color: '#6b6b6b', fontSize: 12 }}>When a trade closes it will appear here with price, notional, and PnL.</div>
          </div>
        )}
        {trades.map((t: any, i: number) => (
          <div key={i} style={{ border: '1px solid var(--border)', borderRadius: 10, padding: '12px', background: '#fff' }}>
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <img src="/meaptrans.png" alt="" style={{ width: 14, height: 14, opacity: .85 }} />
                <img src={`/${(t.symbol||'').replace('USDT','').toLowerCase()}.svg`} alt="" style={{ width: 18, height: 18 }} />
                <div style={{ fontWeight: 800 }}>{t.symbol || '—'}</div>
              </div>
              <div style={{ fontWeight: 800, color: '#1c1c1c' }}>{(t.side||'').toUpperCase()}</div>
            </div>
            <div style={{ color: '#6b6b6b', fontSize: 12, marginTop: 6 }}>{t.reason || '—'}</div>
            <div style={{ color: '#6b6b6b', fontSize: 12 }}>
              {t.openedAt ? new Date(t.openedAt).toLocaleString() : ''}
            </div>
            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6, marginTop: 8, fontSize: 13 }}>
              <div>Price: {t.entryPrice && t.exitPrice ? `${fmtUsd(Number(t.entryPrice))} → ${fmtUsd(Number(t.exitPrice))}` : (t.entryPrice ? `${fmtUsd(Number(t.entryPrice))} → —` : '—')}</div>
              <div>Quantity: {t.qty ?? '—'}</div>
              <div>Notional: {typeof t.notionalEntry !== 'undefined' || typeof t.notionalExit !== 'undefined' ? `${t.notionalEntry?fmtUsd(Number(t.notionalEntry)):'—'} → ${t.notionalExit?fmtUsd(Number(t.notionalExit)):'—'}` : (typeof t.notional !== 'undefined' ? fmtUsd(Number(t.notional)) : '—')}</div>
              <div>Holding time: {t.holdingMs ? fmtHolding(t.holdingMs) : '—'}</div>
            </div>
            <div style={{ marginTop: 8, fontWeight: 800 }}>Net P&L: <span style={{ color: typeof t.pnlUsd==='number' ? (t.pnlUsd>=0?'#166534':'#b91c1c') : '#6b6b6b' }}>{typeof t.pnlUsd === 'number' ? fmtUsd(t.pnlUsd) : '—'}</span></div>
          </div>
        ))}
      </div>
    </>
  );
}

function niceStep(raw: number): number {
  const exp = Math.floor(Math.log10(raw));
  const base = raw / Math.pow(10, exp);
  const niceBase = base < 1.5 ? 1 : base < 3 ? 2 : base < 7 ? 5 : 10;
  return niceBase * Math.pow(10, exp);
}

function fmtNumber(v: number): string {
  if (Math.abs(v) >= 1000) return v.toFixed(0);
  if (Math.abs(v) >= 100) return v.toFixed(0);
  return v.toFixed(2);
}

function fmtUsd(v: number): string { return `$${fmtNumber(v)}`; }

function fmtTime(ts: number): string {
  const d = new Date(ts);
  const mo = d.toLocaleString(undefined, { month: 'short' });
  const dy = String(d.getDate()).padStart(2, '0');
  const hr = String(d.getHours()).padStart(2, '0');
  const mi = String(d.getMinutes()).padStart(2, '0');
  return `${mo} ${dy} ${hr}:${mi}`;
}

function fmtHolding(ms: number): string {
  const m = Math.floor(ms / 60000);
  const h = Math.floor(m / 60);
  const mm = m % 60;
  return `${h}H ${String(mm).padStart(2,'0')}M`;
}
