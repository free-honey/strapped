# Strapped Web

Minimal web UI that polls the indexer once per second and prints the latest snapshot.

## Local dev

```bash
cd web-street
npm install
VITE_INDEXER_URL=https://strapped-indexer-test-net-production.up.railway.app npm run dev
```

## Build

```bash
cd web-street
npm install
VITE_INDEXER_URL=https://strapped-indexer-test-net-production.up.railway.app npm run build
npm run preview
```

## Railway

- Set the service root to `web-street` so Railway picks up `web-street/railpack.json`.
- Add `VITE_INDEXER_URL` in the Railway service variables.
- The build caches `node_modules`, `node_modules/.vite`, and the npm cache to speed deploys.

## Config

- `VITE_INDEXER_URL` must be the base URL for the indexer service.
- The UI requests `GET /snapshot/latest` once per second.
- The indexer must allow CORS from the web UI origin.
