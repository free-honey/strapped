import testDeployment from "../../../.deployments/test/deployments.json";

type DeploymentRecord = {
  network_url: string;
  contract_id: string;
  chip_asset_id?: string;
  chip_asset_ticker?: string;
};

const normalizeGraphqlUrl = (url: string) => {
  const trimmed = url.replace(/\/+$/, "");
  return trimmed.endsWith("/v1/graphql") ? trimmed : `${trimmed}/v1/graphql`;
};

const normalizeHex = (value: string) =>
  value.startsWith("0x") ? value : `0x${value}`;

const deploymentToNetwork = (
  deployment: DeploymentRecord,
  label: string
) => {
  const chipAssetId = deployment.chip_asset_id
    ? normalizeHex(deployment.chip_asset_id)
    : undefined;
  const chipAssetTicker = deployment.chip_asset_ticker;

  if (!chipAssetId || !chipAssetTicker) {
    throw new Error(
      `Deployment record for ${label} is missing chip asset details`
    );
  }

  return {
    label,
    graphqlUrl: normalizeGraphqlUrl(deployment.network_url),
    contractId: normalizeHex(deployment.contract_id),
    chipAssetId,
    chipAssetTicker,
    baseAssetTicker: "ETH",
  };
};

export const FUEL_NETWORKS = {
  testnet: deploymentToNetwork(testDeployment, "Testnet"),
} as const;

export type FuelNetworkKey = keyof typeof FUEL_NETWORKS;

export const DEFAULT_NETWORK = "testnet" satisfies FuelNetworkKey;
