import { useEffect, useMemo, useState } from "react";

const POLL_INTERVAL_MS = 1000;

type FetchStatus = "idle" | "loading" | "ok" | "error";

type SnapshotResponse = unknown;

function normalizeBaseUrl(raw: string | undefined): string {
  if (!raw) {
    return "";
  }
  return raw.replace(/\/$/, "");
}

export default function App() {
  const baseUrl = useMemo(
    () => normalizeBaseUrl(import.meta.env.VITE_INDEXER_URL as string | undefined),
    []
  );
  const [status, setStatus] = useState<FetchStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [data, setData] = useState<SnapshotResponse | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);

  useEffect(() => {
    if (!baseUrl) {
      return;
    }

    let isActive = true;
    let timeoutId: number | undefined;

    const poll = async () => {
      if (!isActive) {
        return;
      }

      setStatus((prev) => (prev === "ok" ? "ok" : "loading"));

      try {
        const response = await fetch(`${baseUrl}/snapshot/latest`, {
          headers: {
            Accept: "application/json",
          },
        });

        if (!response.ok) {
          throw new Error(`indexer responded with ${response.status}`);
        }

        const payload = (await response.json()) as SnapshotResponse;
        if (!isActive) {
          return;
        }

        setData(payload);
        setError(null);
        setStatus("ok");
        setLastUpdated(new Date().toLocaleTimeString());
      } catch (err) {
        if (!isActive) {
          return;
        }
        const message = err instanceof Error ? err.message : "unknown error";
        setError(message);
        setStatus("error");
      } finally {
        if (isActive) {
          timeoutId = window.setTimeout(poll, POLL_INTERVAL_MS);
        }
      }
    };

    poll();

    return () => {
      isActive = false;
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [baseUrl]);

  return (
    <div className="app">
      <header className="app__header">
        <div>
          <div className="app__title">🎲 Strapped Web</div>
          <div className="app__subtitle">Polling the indexer once per second.</div>
        </div>
        <div className="app__status">
          <span className={`pill pill--${status}`}>{status}</span>
          <span className="app__updated">
            {lastUpdated ? `Last update ${lastUpdated}` : "Not updated yet"}
          </span>
        </div>
      </header>

      <section className="panel">
        <h2 className="panel__title">📡 Indexer</h2>
        <div className="panel__body">
          <div className="field">
            <div className="field__label">Base URL</div>
            <div className="field__value">
              {baseUrl || "Set VITE_INDEXER_URL to begin."}
            </div>
          </div>
          {error && (
            <div className="error">
              <span>⚠️</span>
              <span>{error}</span>
            </div>
          )}
          <pre className="payload">
            {data ? JSON.stringify(data, null, 2) : "No payload yet."}
          </pre>
        </div>
      </section>
    </div>
  );
}
