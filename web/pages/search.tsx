import React, { useState } from 'react';
import Image from 'next/image';

const BACKEND_URL = process.env.NEXT_PUBLIC_BACKEND_URL || 'http://localhost:8080';

type SearchHit = {
  doc_id: number;
  score: number;
  title: string;
  url?: string;
  snippet?: string;
};

type SearchResponse = {
  query: string;
  took_ms?: number;
  took_s?: number;
  total_hits: number;
  results: SearchHit[];
};

export default function SearchPage() {
  const [q, setQ] = useState('');
  const [loading, setLoading] = useState(false);
  const [resp, setResp] = useState<SearchResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);

  async function onSubmit(e: React.FormEvent<HTMLFormElement>) {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setResp(null);
    setPage(1);
    try {
      const r = await fetch(`${BACKEND_URL}/search?q=${encodeURIComponent(q)}&k=${Math.min(pageSize, 100)}`);
      const data = (await r.json()) as SearchResponse;
      setResp(data);
    } catch (err: any) {
      setError(err?.message || 'Failed to fetch');
    } finally {
      setLoading(false);
    }
  }

  const tookText = resp?.took_s !== undefined
    ? `${resp.took_s.toFixed(3)} s`
    : resp?.took_ms !== undefined
    ? `${resp.took_ms} ms`
    : '';

  // Pagination (client-side)
  const totalHits = resp?.total_hits ?? 0;
  const effectiveTotal = Math.min(totalHits, 100);
  const totalPages = Math.max(1, Math.ceil(effectiveTotal / pageSize));
  const viewStart = (page - 1) * pageSize;
  const viewEnd = Math.min(viewStart + pageSize, resp?.results.length ?? 0);
  const pageResults = resp ? resp.results.slice(viewStart, viewEnd) : [];

  async function gotoPage(nextPage: number) {
    if (!resp) return;
    const needed = Math.min(nextPage * pageSize, 100);
    setPage(nextPage);
    if (resp.results.length < needed) {
      setLoading(true);
      try {
        const r = await fetch(`${BACKEND_URL}/search?q=${encodeURIComponent(q)}&k=${needed}`);
        const data = (await r.json()) as SearchResponse;
        setResp(data);
      } catch (err: any) {
        setError(err?.message || 'Failed to fetch');
      } finally {
        setLoading(false);
      }
    }
  }

  function onPageSizeChange(newSize: number) {
    setPageSize(newSize);
    setPage(1);
    if (q.trim().length > 0) {
      setLoading(true);
      fetch(`${BACKEND_URL}/search?q=${encodeURIComponent(q)}&k=${Math.min(newSize, 100)}`)
        .then((r) => r.json())
        .then((data: SearchResponse) => setResp(data))
        .catch((err) => setError(err?.message || 'Failed to fetch'))
        .finally(() => setLoading(false));
    }
  }

  return (
    <div className="app-shell">
      {(!resp || resp.results.length === 0) && !loading ? (
        <div className={`hero${(loading || (resp && resp.results)) ? ' exiting' : ''}`}>
          <div className="hero-inner">
            <Image className="hero-logo" src="/brand/zap-logo.png" alt="ZapSearch" width={120} height={120} />
            <form onSubmit={onSubmit} className="search-box hero-search">
              <input
                className="input"
                type="text"
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder="Search the web"
                aria-label="Search"
              />
              <button className="button" type="submit" disabled={loading}>
                {loading ? (<><span className="spinner" /> Searching…</>) : 'Search'}
              </button>
            </form>
            {error && <p className="error" style={{ color: '#ff6b6b', marginTop: 8 }}>{error}</p>}
          </div>
        </div>
      ) : (
        <>
          <div className="header">
            <div style={{ display: 'inline-flex', alignItems: 'center', gap: 8 }}>
              <Image className="brand-logo" src="/brand/zap-logo.png" alt="ZapSearch logo" width={24} height={24} />
              <span className="brand">ZapSearch</span>
            </div>
          </div>
          <form onSubmit={onSubmit} className="search-box">
            <input
              className="input"
              type="text"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Type your query"
            />
            <button className="button" type="submit" disabled={loading}>
              {loading ? (<><span className="spinner" /> Searching…</>) : 'Search'}
            </button>
          </form>

          {error && <p className="error" style={{ color: '#ff6b6b' }}>{error}</p>}

          {resp && (
          <div className={`results-shell visible`}>
            <div className="meta">
              {resp.results.length === 0 ? 'No results' : `Showing ${viewStart + 1}-${viewEnd} of ${resp.total_hits}`} • {tookText}
            </div>
            <div className="pagination" style={{ display: 'flex', gap: 8, alignItems: 'center', margin: '8px 0' }}>
              <button className="button" disabled={page <= 1 || loading} onClick={() => gotoPage(page - 1)}>Prev</button>
              <span className="page-indicator">Page {page} / {totalPages}</span>
              <button className="button" disabled={page >= totalPages || loading} onClick={() => gotoPage(page + 1)}>Next</button>
              <span className="meta" style={{ marginLeft: 8 }}>Per page:</span>
              <select
                className="input"
                value={pageSize}
                onChange={(e) => onPageSizeChange(parseInt(e.target.value, 10))}
                style={{ width: 88 }}
              >
                <option value={10}>10</option>
                <option value={20}>20</option>
                <option value={50}>50</option>
              </select>
            </div>
            <ul className="results">
              {pageResults.map((r, i) => (
                <li key={r.doc_id} className="item fade-in-up" style={{ animationDelay: `${i * 40}ms` }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12 }}>
                    <h3>
                      {r.url ? (
                        <a className="title-link" href={r.url} target="_blank" rel="noreferrer">{r.title}</a>
                      ) : (
                        r.title
                      )}
                    </h3>
                    <span className="score">{r.score.toFixed(4)}</span>
                  </div>
                  {r.url && (
                    <div className="url-badge">
                      <a href={r.url} target="_blank" rel="noreferrer">{r.url}</a>
                    </div>
                  )}
                  {r.snippet && <p className="snippet" dangerouslySetInnerHTML={{ __html: r.snippet }} />}
                </li>
              ))}
            </ul>
            <div className="footer-note">Powered by a Rust TF‑IDF engine • Client: Next.js</div>
          </div>
          )}
        </>
      )}
    </div>
  );
}
