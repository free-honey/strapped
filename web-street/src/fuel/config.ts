export const FUEL_NETWORKS = {
  testnet: {
    label: "Testnet",
    graphqlUrl: "https://testnet.fuel.network/v1/graphql",
    contractId:
      "0xbfd7d193d76ba362034a60276201adb83540b11b4e98ee69df24309a883db9c8",
    chipAssetId:
      "0xf8f8b6283d7fa5b672b530cbb84fcccb4ff8dc40f8176ef4544ddb1f1952ad07",
  },
} as const;

export type FuelNetworkKey = keyof typeof FUEL_NETWORKS;

export const DEFAULT_NETWORK = "testnet" satisfies FuelNetworkKey;
