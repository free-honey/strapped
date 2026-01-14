import { Contract, WalletUnlocked } from "fuels";
import strappedAbi from "../abi/strapped-abi.json";
import { DEFAULT_NETWORK, FUEL_NETWORKS, FuelNetworkKey } from "./config";

export const createStrappedContract = (
  wallet: WalletUnlocked,
  network: FuelNetworkKey = DEFAULT_NETWORK
) => new Contract(FUEL_NETWORKS[network].contractId, strappedAbi, wallet);
