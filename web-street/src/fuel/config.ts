type DeploymentResponse = {
  network_url: string;
  contract_id: string;
  chip_asset_id: string;
  chip_asset_ticker: string;
};

export type FuelNetworkConfig = {
  label: string;
  graphqlUrl: string;
  contractId: string;
  chipAssetId: string;
  chipAssetTicker: string;
  baseAssetTicker: string;
};

export type FuelNetworks = {
  testnet: FuelNetworkConfig;
};

export type FuelNetworkKey = keyof FuelNetworks;

export const DEFAULT_NETWORK = "testnet" satisfies FuelNetworkKey;

const normalizeGraphqlUrl = (url: string) => {
  const trimmed = url.replace(/\/+$/, "");
  return trimmed.endsWith("/v1/graphql") ? trimmed : `${trimmed}/v1/graphql`;
};

const normalizeHex = (value: string) =>
  value.startsWith("0x") ? value : `0x${value}`;

const joinUrl = (base: string, path: string) => {
  const trimmed = base.replace(/\/+$/, "");
  return path.startsWith("/") ? `${trimmed}${path}` : `${trimmed}/${path}`;
};

const deploymentToNetwork = (
  deployment: DeploymentResponse,
  label: string
): FuelNetworkConfig => {
  const chipAssetId = normalizeHex(deployment.chip_asset_id);
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

export const fetchFuelNetworks = async (
  indexerBaseUrl: string
): Promise<FuelNetworks> => {
  if (!indexerBaseUrl) {
    throw new Error("Indexer URL is required to load deployment config");
  }

  const response = await fetch(joinUrl(indexerBaseUrl, "/deployment"));
  if (!response.ok) {
    throw new Error(`Indexer responded with ${response.status}`);
  }

  const deployment = (await response.json()) as DeploymentResponse;
  return {
    testnet: deploymentToNetwork(deployment, "Testnet"),
  };
};
