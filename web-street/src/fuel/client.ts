import { Contract, Provider, WalletUnlocked } from "fuels";
import strappedAbi from "../abi/strapped-abi.json";
import { DEFAULT_NETWORK, FUEL_NETWORKS, FuelNetworkKey } from "./config";

type FuelApi = {
  connect: () => Promise<boolean>;
  isConnected?: () => Promise<boolean>;
  accounts: () => Promise<string[]>;
  currentAccount?: () => Promise<string | null>;
  getWallet: (address?: string) => Promise<WalletUnlocked>;
};

type FuelWindow = Window & { fuel?: FuelApi };

export type ConnectedWallet = {
  wallet: WalletUnlocked;
  address: string;
  network: FuelNetworkKey;
};

const getFuelApi = (): FuelApi | null => {
  if (typeof window === "undefined") {
    return null;
  }

  const fuel =
    (window as FuelWindow).fuel ??
    (window as unknown as { Fuel?: FuelApi }).Fuel;
  if (fuel) {
    console.debug("[fuel] wallet API detected");
  } else {
    console.debug("[fuel] wallet API not present on window");
  }
  return fuel ?? null;
};

const waitForFuelApi = (timeoutMs = 2000): Promise<FuelApi | null> =>
  new Promise((resolve) => {
    const existing = getFuelApi();
    if (existing) {
      resolve(existing);
      return;
    }

    if (typeof window === "undefined") {
      resolve(null);
      return;
    }

    console.debug("[fuel] waiting for wallet API", { timeoutMs });

    let resolved = false;
    let intervalId: number | undefined;
    let timeoutId: number | undefined;

    const finalize = (value: FuelApi | null) => {
      if (resolved) {
        return;
      }
      resolved = true;
      console.debug("[fuel] wallet API resolved", {
        found: Boolean(value),
      });
      if (intervalId !== undefined) {
        window.clearInterval(intervalId);
      }
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
      window.removeEventListener("fuel#initialized", handleReady);
      resolve(value);
    };

    const handleReady = () => {
      const fuel = getFuelApi();
      if (fuel) {
        finalize(fuel);
      }
    };

    window.addEventListener("fuel#initialized", handleReady);
    intervalId = window.setInterval(handleReady, 200);
    timeoutId = window.setTimeout(() => finalize(null), timeoutMs);
  });

const resolveAccount = async (fuel: FuelApi): Promise<string | null> => {
  if (fuel.currentAccount) {
    const account = await fuel.currentAccount();
    if (account) {
      return account;
    }
  }

  const accounts = await fuel.accounts();
  return accounts[0] ?? null;
};

export const connectFuelWallet = async (
  network: FuelNetworkKey = DEFAULT_NETWORK
): Promise<ConnectedWallet> => {
  const fuel = await waitForFuelApi();
  if (!fuel) {
    throw new Error(
      "Fuel wallet extension not detected. Make sure it is enabled for this site."
    );
  }

  if (fuel.isConnected) {
    const isConnected = await fuel.isConnected();
    if (!isConnected) {
      await fuel.connect();
    }
  } else {
    await fuel.connect();
  }

  const address = await resolveAccount(fuel);
  if (!address) {
    throw new Error("No Fuel accounts available");
  }

  const wallet = await fuel.getWallet(address);
  const provider = new Provider(FUEL_NETWORKS[network].graphqlUrl);
  wallet.connect(provider);

  return { wallet, address, network };
};

export const createStrappedContract = (
  wallet: WalletUnlocked,
  network: FuelNetworkKey = DEFAULT_NETWORK
) => new Contract(FUEL_NETWORKS[network].contractId, strappedAbi, wallet);
