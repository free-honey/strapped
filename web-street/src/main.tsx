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
import { FUEL_NETWORKS } from "./fuel/config";

const rootElement = document.getElementById("root");
const queryClient = new QueryClient();

if (!rootElement) {
  throw new Error("Missing root element");
}

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
            url: FUEL_NETWORKS.testnet.graphqlUrl,
            name: FUEL_NETWORKS.testnet.label,
          },
        ]}
      >
        <App />
      </FuelProvider>
    </QueryClientProvider>
  </React.StrictMode>
);
