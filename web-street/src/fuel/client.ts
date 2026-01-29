import { Contract } from "fuels";
import type { Account } from "fuels";
import strappedAbi from "../abi/strapped-abi.json";

export const createStrappedContract = (wallet: Account, contractId: string) =>
  new Contract(contractId, strappedAbi, wallet);
