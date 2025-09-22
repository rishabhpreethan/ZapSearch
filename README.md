# search-engine-rs

Rust TF-IDF inverted-index search engine with an Axum HTTP server and a Next.js frontend.

This repository is structured as a Cargo workspace:

- core/ — library: tokenization, index structures, TF-IDF math, persistence helpers, tests
- indexer/ — CLI: batch indexer that ingests JSON/JSONL and writes index/ to disk
- server/ — Axum HTTP server exposing /search and /doc/{id}
- web/ — Next.js app, can run locally or be deployed to Vercel

## Quick start

```
cargo build
```

## Crawl Top-10k (optional)

Create a seeds file using Tranco (one domain per line):
```
python3 -m venv .venv
source .venv/bin/activate
pip install tranco
python -c "from tranco import Tranco; l = Tranco(cache=True).list(); print('\n'.join(l.top(10000)))" > seeds_top10k.txt
```

Run the crawler (polite defaults; respects robots.txt):
```
cargo run -p crawler -- \
  --seeds ./seeds_top10k.txt \
  --output ./sample_data/crawl_top10k.jsonl \
  --max-docs 10000 \
  --same-host-only \
  --max-per-host 10 \
  --concurrency 16 \
  --timeout-secs 8 | tee crawl_10k.log
```

Validate and clean JSONL (recommended):
```
python - << 'PY'
import json, sys
inp='sample_data/crawl_top10k.jsonl'; out='sample_data/crawl_top10k.cleaned.jsonl'; bad=0
with open(inp,'r',encoding='utf-8',errors='replace') as f, open(out,'w',encoding='utf-8') as o:
    for i,line in enumerate(f,1):
        line=line.strip()
        if not line: continue
        try:
            obj=json.loads(line); json.dump(obj,o,ensure_ascii=False); o.write('\n')
        except Exception as e:
            bad+=1; print(f'BAD line {i}: {e}', file=sys.stderr)
print(f'Done. Bad lines: {bad}', file=sys.stderr)
PY
```

Run server:
```
cargo run -p server -- --index ./index --host 0.0.0.0 --port 8080
```

## Prerequisites

- Rust toolchain via rustup
- Node.js 18+ for the Next.js web app
- Docker (optional, for containerized backend)

## Data formats

Input documents (JSON/JSONL):
```
{
  "id": "uuid-or-int-string",
  "title": "Short title",
  "body": "Full text to index",
  "url": "https://...",
  "timestamp": "2024-01-01T12:00:00Z",
  "meta": { "author": "X" }
}
```

Index directory layout (`./index/`):
- `meta.json` — `{ num_docs: N, created_at: ..., version: 1 }`
- `dictionary.bin` — bincode(HashMap<String, TermId>, Vec<u32> df)
- `docs.bin` — bincode(HashMap<DocId, DocMeta>)
- `doc_id_map.bin` — bincode(HashMap<String, DocId>)
- `postings/{term_id:08}.postings.bin` — bincode(Vec<Posting { doc_id, weight }>)
- `texts/{doc_id}.txt` — raw text for snippets

Weights are normalized TF-IDF: `weight = ( (1+ln(tf)) * ln(N/df) ) / doc_norm`.

## Build & index

Generate sample docs (optional):
``` 
rustc sample_data/make_sample_data.rs -O -o /tmp/make_sample_data
/tmp/make_sample_data 5000 > sample_data/docs.jsonl
```

Build the index:
```
cargo run -p indexer -- build --input ./sample_data/crawl_top10k.cleaned.jsonl --output ./index
```

## Run the server

```
cargo run -p server -- --index ./index --host 0.0.0.0 --port 8080
```

Healthcheck:
```
curl http://localhost:8080/health
```

Search:
```
curl 'http://localhost:8080/search?q=rust+inverted+index&k=5'
```

Doc by id:
```
curl 'http://localhost:8080/doc/0'
```

Admin (stubs): set `ADMIN_TOKEN` env and pass `X-ADMIN-TOKEN` header.

## Web frontend

```
cd web
npm i
export NEXT_PUBLIC_BACKEND_URL=http://localhost:8080
npm run dev
# open http://localhost:3000/search
```

Set `NEXT_PUBLIC_BACKEND_URL` in Vercel to your backend URL.

## API spec

- `GET /search?q=terms&k=10`
  - Response:
  ```json
  {
    "query": "terms",
    "took_s": 0.012,
    "took_ms": 12,
    "total_hits": 123,
    "results": [
      { "doc_id": 12, "score": 0.8234, "title": "...", "url": "...", "snippet": "... <em>term</em> ..." }
    ]
  }
  ```

- `GET /doc/{id}`
  - Returns stored metadata and optionally full text.

## Docker

Build image:
```
docker build -t search-engine .
```

Run container (mount index):
```
docker run --rm -p 8080:8080 -v $(pwd)/index:/data/index -e PORT=8080 search-engine
```

docker-compose for local dev:
```
docker compose up --build
```

## Tests & Benchmarks

- Unit tests: `cargo test` (e.g., tokenizer tests in `core/tests/`)
- Benchmarks: `cargo bench` (criterion bench for tokenizer)

## Security & Ops Notes

- Protect admin endpoints with `X-ADMIN-TOKEN` header and `ADMIN_TOKEN` env var.
- Enable CORS appropriately (currently permissive; restrict in production).
- Health endpoint: `/health`.
- Target hardware: 100k docs fits in a few hundred MB depending on vocabulary; postings are loaded on demand to reduce RAM.
