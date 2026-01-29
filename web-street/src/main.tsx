import {
  FuelWalletConnector,
  FuelWalletDevelopmentConnector,
  FueletWalletConnector,
} from "@fuels/connectors";
import { FuelProvider } from "@fuels/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import React from "react";
import { createRoot } from "react-dom/client";
import { CHAIN_IDS } from "fuels";
import App from "./App";
import "./app.css";
import { fetchFuelNetworks } from "./fuel/config";

const rootElement = document.getElementById("root");
const queryClient = new QueryClient();

if (!rootElement) {
  throw new Error("Missing root element");
}

const normalizeBaseUrl = (url: string | undefined) => {
  if (!url) {
    return null;
  }
  return url.replace(/\/+$/, "");
};

const renderError = (message: string) => {
  createRoot(rootElement).render(
    <React.StrictMode>
      <div style={{ padding: "24px", fontFamily: "sans-serif" }}>
        <h1>Failed to start</h1>
        <p>{message}</p>
      </div>
    </React.StrictMode>
  );
};

const bootstrap = async () => {
  const baseUrl = normalizeBaseUrl(
    import.meta.env.VITE_INDEXER_URL as string | undefined
  );
  if (!baseUrl) {
    renderError("VITE_INDEXER_URL is not set");
    return;
  }
  try {
    const networks = await fetchFuelNetworks(baseUrl);
    createRoot(rootElement).render(
      <React.StrictMode>
        <QueryClientProvider client={queryClient}>
          <FuelProvider
            fuelConfig={{
              connectors: [
                new FuelWalletConnector(),
                new FuelWalletDevelopmentConnector(),
                new FueletWalletConnector(),
              ],
            }}
            networks={[
              {
                chainId: CHAIN_IDS.fuel.testnet,
                url: networks.testnet.graphqlUrl,
              },
            ]}
          >
            <App />
          </FuelProvider>
        </QueryClientProvider>
      </React.StrictMode>
    );
  } catch (err) {
    renderError((err as Error).message);
  }
};

bootstrap();
