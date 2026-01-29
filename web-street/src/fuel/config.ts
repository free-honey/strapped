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

const deploymentOverrides = {
  ...import.meta.glob("../../../.deployments/**/deployments.json", {
    eager: true,
  }),
  ...import.meta.glob("./deployments.test.json", { eager: true }),
} as Record<string, { default: DeploymentRecord }>;

const resolveTestDeployment = (): DeploymentRecord => {
  const overridePath = import.meta.env.VITE_DEPLOYMENTS_PATH;
  const normalizedInput = overridePath?.replace(/^(\.\.\/)+/, "");

  if (overridePath) {
    const candidates = [
      normalizedInput?.startsWith(".deployments/")
        ? `../../../${normalizedInput}`
        : overridePath,
      normalizedInput?.startsWith(".deployments/")
        ? `../../../${normalizedInput}`
        : undefined,
    ].filter(Boolean) as string[];

    for (const candidate of candidates) {
      const match = deploymentOverrides[candidate];
      if (match) {
        return match.default;
      }
    }

    throw new Error(
      `VITE_DEPLOYMENTS_PATH did not match any bundled deployments.json: ${overridePath}`
    );
  }

  const defaultMatch =
    deploymentOverrides["../../../.deployments/test/deployments.json"] ??
    deploymentOverrides["./deployments.test.json"];

  if (!defaultMatch) {
    throw new Error(
      "No deployments.json found. Set VITE_DEPLOYMENTS_PATH or include deployments.test.json."
    );
  }

  return defaultMatch.default;
};

export const FUEL_NETWORKS = {
  testnet: deploymentToNetwork(resolveTestDeployment(), "Testnet"),
} as const;

export type FuelNetworkKey = keyof typeof FUEL_NETWORKS;

export const DEFAULT_NETWORK = "testnet" satisfies FuelNetworkKey;
