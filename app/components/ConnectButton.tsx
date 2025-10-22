"use client";

import { useEffect, useMemo, useState } from 'react';

type InjectedProvider = any;

type DetectedWallet = {
  id: string;
  name: string;
  provider: InjectedProvider;
};

const BSC_PARAMS = {
  chainId: '0x38', // 56
  chainName: 'BNB Smart Chain',
  nativeCurrency: { name: 'BNB', symbol: 'BNB', decimals: 18 },
  rpcUrls: ['https://bsc-dataseed.binance.org'],
  blockExplorerUrls: ['https://bscscan.com']
};

function detectProviders(): InjectedProvider[] {
  const eth = (window as any).ethereum;
  if (!eth) return [];
  if (eth.providers && Array.isArray(eth.providers)) return eth.providers as InjectedProvider[];
  return [eth];
}

function labelFor(p: InjectedProvider): string {
  // Order matters for MetaMask vs Brave
  if (p.isRabby) return 'Rabby';
  if (p.isTrustWallet) return 'Trust Wallet';
  if (p.isCoinbaseWallet) return 'Coinbase Wallet';
  if (p.isOkxWallet || p.isOKExWallet) return 'OKX Wallet';
  if (p.isBraveWallet) return 'Brave Wallet';
  if (p.isMetaMask) return 'MetaMask';
  return 'Injected Wallet';
}

function idFor(p: InjectedProvider): string {
  if (p.isRabby) return 'rabby';
  if (p.isTrustWallet) return 'trust';
  if (p.isCoinbaseWallet) return 'coinbase';
  if (p.isOkxWallet || p.isOKExWallet) return 'okx';
  if (p.isBraveWallet) return 'brave';
  if (p.isMetaMask) return 'metamask';
  return 'injected';
}

export default function ConnectButton() {
  const [account, setAccount] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [providers, setProviders] = useState<InjectedProvider[]>([]);
  const wallets: DetectedWallet[] = useMemo(() => {
    return providers.map((p: InjectedProvider) => ({ id: idFor(p), name: labelFor(p), provider: p }));
  }, [providers]);

  useEffect(() => {
    if (typeof window !== 'undefined') {
      try { setProviders(detectProviders()); } catch { /* noop */ }
    }
  }, []);

  async function ensureBsc(p: InjectedProvider) {
    try {
      await p.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: BSC_PARAMS.chainId }] });
    } catch (err: any) {
      if (err?.code === 4902) {
        await p.request({ method: 'wallet_addEthereumChain', params: [BSC_PARAMS] });
      } else {
        // ignore
      }
    }
  }

  async function connectWith(provider: InjectedProvider) {
    try {
      const [addr] = await provider.request({ method: 'eth_requestAccounts' });
      await ensureBsc(provider);
      setAccount(addr);
      try {
        (window as any).meapAccount = addr;
        localStorage.setItem('meap_account', addr);
        window.dispatchEvent(new CustomEvent('meap:account', { detail: { address: addr } }));
        provider.on?.('accountsChanged', (accs: string[]) => {
          const a = accs?.[0];
          if (a) {
            (window as any).meapAccount = a;
            localStorage.setItem('meap_account', a);
            window.dispatchEvent(new CustomEvent('meap:account', { detail: { address: a } }));
            setAccount(a);
          }
        });
      } catch {}
      setOpen(false);
    } catch (e) {
      console.error(e);
    }
  }

  useEffect(() => {
    const btn = document.getElementById('connectButton');
    if (!btn) return;
    btn.onclick = () => {
      if (wallets.length <= 1) {
        const provider = wallets[0]?.provider || (window as any).ethereum;
        if (!provider) return alert('No EVM wallet detected');
        connectWith(provider);
      } else {
        setOpen(true);
      }
    };
  }, [wallets]);

  useEffect(() => {
    const btn = document.getElementById('connectButton');
    if (btn && account) btn.textContent = `${account.slice(0, 6)}...${account.slice(-4)}`;
  }, [account]);

  // restore on load
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const saved = localStorage.getItem('meap_account');
    if (saved) {
      (window as any).meapAccount = saved;
      setAccount(saved);
    }
  }, []);

  if (!open) return null;

  return (
    <div className="modal" onClick={() => setOpen(false)}>
      <div className="modal-content" onClick={(e) => e.stopPropagation()}>
        <h3 style={{ marginTop: 0 }}>Connect Wallet</h3>
        <p style={{ color: '#475569', marginTop: 4 }}>Select an EVM wallet (BNB supported).</p>
        <div className="wallet-grid">
          {wallets.map((w) => (
            <button key={w.id} className="wallet-item" onClick={() => connectWith(w.provider)}>
              <span>{w.name}</span>
            </button>
          ))}
          {wallets.length === 0 && (
            <div style={{ color: '#475569' }}>No wallet detected. Please install Rabby, MetaMask, Trust, OKX, or Coinbase Wallet.</div>
          )}
        </div>
        <button className="btn-close" onClick={() => setOpen(false)}>Close</button>
      </div>
    </div>
  );
}
