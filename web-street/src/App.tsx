import {
  useAccount,
  useBalance,
  useConnectUI,
  useIsConnected,
  useProvider,
  useSelectNetwork,
  useWallet,
} from "@fuels/react";
import { CSSProperties, useEffect, useMemo, useRef, useState } from "react";
import { createStrappedContract } from "./fuel/client";
import { DEFAULT_NETWORK, FUEL_NETWORKS, FuelNetworkKey } from "./fuel/config";

const POLL_INTERVAL_MS = 1000;

type FetchStatus = "idle" | "loading" | "ok" | "error";

type Roll =
  | "Two"
  | "Three"
  | "Four"
  | "Five"
  | "Six"
  | "Seven"
  | "Eight"
  | "Nine"
  | "Ten"
  | "Eleven"
  | "Twelve";

type Strap = {
  level: number;
  kind: string;
  modifier: string;
};

type ModifierShopEntry = {
  trigger_roll: Roll;
  modifier_roll: Roll;
  modifier: string;
  triggered?: boolean;
  purchased?: boolean;
  price: number;
};

type TableAccountBets = {
  identity: unknown;
  per_roll_bets: [Roll, [unknown, number, number][]][];
};

type OverviewSnapshot = {
  game_id: number;
  rolls: Roll[];
  pot_size: number;
  chips_owed: number;
  current_block_height: number;
  next_roll_height: number | null;
  roll_frequency?: number | null;
  first_roll_height?: number | null;
  rewards: [Roll, Strap, number][];
  total_chip_bets: number;
  specific_bets: [number, [Strap, number][]][];
  modifiers_active: (string | null)[];
  modifier_shop: ModifierShopEntry[];
  table_bets: TableAccountBets[];
};

type SnapshotResponse = {
  snapshot: OverviewSnapshot;
  block_height: number;
};

type AccountSnapshot = {
  total_chip_bet: number;
  strap_bets: [Strap, number][];
  total_chip_won: number;
  claimed_rewards: [number, [Strap, number][]] | null;
  per_roll_bets: NormalizedRollBets[];
};

type AccountSnapshotResponse = {
  snapshot: AccountSnapshot;
  block_height: number;
};

type StrapMetadata = {
  assetId: string;
  strap: Strap;
};

type HistoricalSnapshot = {
  game_id: number;
  rolls: Roll[];
  modifiers: { rollIndex: number; modifier: string; modifierRoll: Roll }[];
  strap_rewards: [Roll, Strap, number][];
};

type HistoryEntry = {
  gameId: number;
  rolls: Roll[];
  modifiers: { rollIndex: number; modifier: string; modifierRoll: Roll }[];
  strapRewards: [Roll, Strap, number][];
  account: AccountSnapshot | null;
  claimed: boolean;
};

type ClaimResult = {
  gameId: number;
  chipWon: number | null;
  chipBet: number | null;
  netChip: number | null;
  betDetails: Array<{
    roll: Roll;
    kind: "chip" | "strap";
    amount: number;
    strap?: Strap;
    betRollIndex?: number;
  }>;
  straps: [Strap, number][];
  txId: string | null;
};

type OwnedStrap = {
  assetId: string;
  strap: Strap;
  amount: string;
};

type AccountBetDetail =
  | { amount: number; kind: "chip"; betRollIndex?: number }
  | { amount: number; kind: "strap"; strap: Strap; betRollIndex?: number };

const rollOrder: Roll[] = [
  "Two",
  "Three",
  "Four",
  "Five",
  "Six",
  "Seven",
  "Eight",
  "Nine",
  "Ten",
  "Eleven",
  "Twelve",
];

const rollLabels: Record<Roll, string> = {
  Two: "Two",
  Three: "Three",
  Four: "Four",
  Five: "Five",
  Six: "Six",
  Seven: "Seven/RESET",
  Eight: "Eight",
  Nine: "Nine",
  Ten: "Ten",
  Eleven: "Eleven",
  Twelve: "Twelve",
};

const rollNumbers: Record<Roll, number> = {
  Two: 2,
  Three: 3,
  Four: 4,
  Five: 5,
  Six: 6,
  Seven: 7,
  Eight: 8,
  Nine: 9,
  Ten: 10,
  Eleven: 11,
  Twelve: 12,
};

const strapEmojis: Record<string, string> = {
  Shirt: "👕",
  Pants: "👖",
  Shoes: "👟",
  Dress: "👗",
  Hat: "🎩",
  Glasses: "👓",
  Watch: "⌚",
  Ring: "💍",
  Necklace: "📿",
  Earring: "🧷",
  Bracelet: "🧶",
  Tattoo: "🐉",
  Skirt: "👚",
  Piercing: "📌",
  Coat: "🧥",
  Scarf: "🧣",
  Gloves: "🧤",
  Gown: "👘",
  Belt: "🧵",
};

const strapKindOrder = Object.keys(strapEmojis);

const modifierEmojis: Record<string, string> = {
  Nothing: "",
  Burnt: "🧯",
  Lucky: "🍀",
  Holy: "👼",
  Holey: "🫥",
  Scotch: "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
  Soaked: "🌊",
  Moldy: "🍄",
  Starched: "🏳️",
  Evil: "😈",
  Groovy: "✌️",
  Delicate: "❤️",
};

const modifierOrder = Object.keys(modifierEmojis);

type ModifierStory = {
  cta: string;
  applied: string;
  icon: string;
  theme: string;
};

const modifierStories: Record<string, ModifierStory> = {
  Burnt: { cta: "commit arson", applied: "ablaze", icon: "🧯", theme: "burnt" },
  Lucky: { cta: "bury gold", applied: "charmed", icon: "🍀", theme: "lucky" },
  Holy: { cta: "buy indulgences", applied: "blessed", icon: "👼", theme: "holy" },
  Holey: { cta: "release moths", applied: "overrun", icon: "🫥", theme: "holey" },
  Scotch: { cta: "play bagpipes", applied: "bagpipes playing softly", icon: "🏴󠁧󠁢󠁳󠁣󠁴󠁿", theme: "scotch" },
  Soaked: { cta: "open hydrant", applied: "flooded", icon: "🌊", theme: "soaked" },
  Moldy: {
    cta: "leave clothes in washer",
    applied: "mildewed",
    icon: "🍄",
    theme: "moldy",
  },
  Starched: { cta: "dump flour", applied: "dusted", icon: "🏳️", theme: "starched" },
  Evil: { cta: "curse shop", applied: "cursed", icon: "😈", theme: "evil" },
  Groovy: { cta: "dump paint", applied: "splattered", icon: "✌️", theme: "groovy" },
  Delicate: {
    cta: "handle with care",
    applied: "caressed",
    icon: "❤️",
    theme: "delicate",
  },
};

const modifierClassNames: Record<string, string> = {
  Burnt: "burnt",
  Lucky: "lucky",
  Holy: "holy",
  Holey: "holey",
  Scotch: "scotch",
  Soaked: "soaked",
  Moldy: "moldy",
  Starched: "starched",
  Evil: "evil",
  Groovy: "groovy",
  Delicate: "delicate",
};

function normalizeBaseUrl(raw: string | undefined): string {
  if (!raw) {
    return "";
  }
  return raw.replace(/\/$/, "");
}

function normalizeModifierShop(entries: unknown): ModifierShopEntry[] {
  if (!Array.isArray(entries)) {
    return [];
  }

  return entries
    .map((entry) => {
      if (Array.isArray(entry) && entry.length === 6) {
        const [trigger_roll, modifier_roll, modifier, triggered, purchased, price] =
          entry;
        return {
          trigger_roll: trigger_roll as Roll,
          modifier_roll: modifier_roll as Roll,
          modifier: modifier as string,
          triggered: Boolean(triggered),
          purchased: Boolean(purchased),
          price: Number(price),
        };
      }

      if (entry && typeof entry === "object") {
        const obj = entry as Record<string, unknown>;
        if ("trigger_roll" in obj && "modifier_roll" in obj) {
          return obj as ModifierShopEntry;
        }
      }

      return null;
    })
    .filter((entry): entry is ModifierShopEntry => entry !== null);
}

function formatIdentity(identity: unknown): string {
  if (!identity || typeof identity !== "object") {
    return String(identity ?? "unknown");
  }

  const record = identity as Record<string, unknown>;
  if (typeof record.Address === "string") {
    return record.Address;
  }
  if (typeof record.ContractId === "string") {
    return record.ContractId;
  }

  return JSON.stringify(identity);
}

type NormalizedRollBets = {
  roll: Roll;
  bets: unknown[];
};

function normalizePerRollBets(input: unknown): NormalizedRollBets[] {
  if (!Array.isArray(input)) {
    return [];
  }

  return input
    .map((entry) => {
      if (Array.isArray(entry) && entry.length === 2) {
        const [roll, bets] = entry;
        return {
          roll: roll as Roll,
          bets: Array.isArray(bets) ? bets : [],
        };
      }

      if (entry && typeof entry === "object") {
        const obj = entry as Record<string, unknown>;
        if ("roll" in obj && "bets" in obj) {
          return {
            roll: obj.roll as Roll,
            bets: Array.isArray(obj.bets) ? obj.bets : [],
          };
        }
      }

      return null;
    })
    .filter((entry): entry is NormalizedRollBets => entry !== null);
}

type BetSummary = {
  chipTotal: number;
  straps: [Strap, number][];
};

const strapKey = (strap: Strap) =>
  `${strap.kind}:${strap.level}:${strap.modifier}`;

const parseStrap = (value: unknown): Strap | null => {
  if (!value || typeof value !== "object") {
    return null;
  }

  const record = value as Record<string, unknown>;
  if (
    typeof record.level === "number" &&
    typeof record.kind === "string" &&
    typeof record.modifier === "string"
  ) {
    return {
      level: record.level,
      kind: record.kind,
      modifier: record.modifier,
    };
  }

  return null;
};

const parseBetKind = (
  value: unknown
): { kind: "chip" } | { kind: "strap"; strap: Strap } | null => {
  if (value === "Chip") {
    return { kind: "chip" };
  }
  if (value === "Strap") {
    return null;
  }
  if (Array.isArray(value)) {
    const match = value
      .map((entry) => parseBetKind(entry))
      .find((entry) => entry !== null);
    return match ?? null;
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if ("Chip" in record) {
      return { kind: "chip" };
    }
    if ("Strap" in record) {
      const strap = parseStrap(record.Strap);
      return strap ? { kind: "strap", strap } : null;
    }
  }
  return null;
};

const parseBetPlacement = (
  bet: unknown
): { amount: number; kind: "chip" } | { amount: number; kind: "strap"; strap: Strap } | null => {
  if (!bet) {
    return null;
  }

  const record = bet as Record<string, unknown>;
  const amount =
    typeof record.amount === "number"
      ? record.amount
      : Array.isArray(bet) && typeof bet[1] === "number"
        ? bet[1]
        : Array.isArray(bet)
          ? bet.find((value) => typeof value === "number")
          : null;

  if (typeof amount !== "number") {
    return null;
  }

  const kindCandidate =
    "kind" in record
      ? record.kind
      : "bet" in record
        ? record.bet
        : bet;
  const kind = parseBetKind(kindCandidate);
  if (kind?.kind === "chip") {
    return { amount, kind: "chip" };
  }
  if (kind?.kind === "strap") {
    return { amount, kind: "strap", strap: kind.strap };
  }

  const fallbackStrap =
    "strap" in record ? parseStrap(record.strap) : parseStrap(record.Strap);
  if (fallbackStrap) {
    return { amount, kind: "strap", strap: fallbackStrap };
  }

  return null;
};

type BetTotals = {
  chipTotal: number;
  strapTotals: Map<string, { strap: Strap; amount: number }>;
};

const summarizeBetsByKind = (bets: unknown[]): BetSummary => {
  const initialTotals: BetTotals = {
    chipTotal: 0,
    strapTotals: new Map<string, { strap: Strap; amount: number }>(),
  };
  const totals = bets.reduce<BetTotals>(
    (acc, bet) => {
      const parsed = parseBetPlacement(bet);
      if (!parsed) {
        return acc;
      }
      if (parsed.kind === "chip") {
        acc.chipTotal += parsed.amount;
        return acc;
      }

      const key = strapKey(parsed.strap);
      const existing = acc.strapTotals.get(key);
      acc.strapTotals.set(key, {
        strap: parsed.strap,
        amount: (existing?.amount ?? 0) + parsed.amount,
      });
      return acc;
    },
    initialTotals
  );

  return {
    chipTotal: totals.chipTotal,
    straps: Array.from(totals.strapTotals.values()).map((entry) => [
      entry.strap,
      entry.amount,
    ]),
  };
};

const formatAssetId = (assetId: string | null | undefined) => {
  if (!assetId) {
    return "—";
  }
  const normalized = assetId.startsWith("0x") ? assetId.slice(2) : assetId;
  const prefix = normalized.slice(0, 4);
  return `0x${prefix}...`;
};

const normalizeAssetIdValue = (assetId: string | null | undefined) => {
  if (!assetId) {
    return null;
  }
  const lower = assetId.toLowerCase();
  return lower.startsWith("0x") ? lower : `0x${lower}`;
};

const formatAssetLabel = (assetId: string | null | undefined, ticker?: string) => {
  const prefix = formatAssetId(assetId);
  if (prefix === "—") {
    return prefix;
  }
  return ticker ? `${ticker} | ${prefix}` : prefix;
};

const formatBalanceValue = (value: unknown) => {
  if (value === null || value === undefined) {
    return "—";
  }
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number") {
    return value.toString();
  }
  const record = value as { format?: () => string; toString?: () => string };
  if (typeof record.format === "function") {
    return record.format();
  }
  if (typeof record.toString === "function") {
    return record.toString();
  }
  return "—";
};

const formatChipUnits = (value: unknown) => {
  if (value === null || value === undefined) {
    return "—";
  }
  const record = value as { toString?: () => string };
  if (typeof record.toString !== "function") {
    return "—";
  }
  try {
    const raw = BigInt(record.toString());
    return raw.toLocaleString("en-US");
  } catch (err) {
    return record.toString();
  }
};

const formatQuantity = (value: unknown) => {
  if (value === null || value === undefined) {
    return "—";
  }
  const record = value as { toString?: () => string };
  if (typeof record.toString !== "function") {
    return "—";
  }
  try {
    const raw = BigInt(record.toString());
    return raw.toLocaleString("en-US");
  } catch (err) {
    return record.toString();
  }
};

const parseAccountBetPlacement = (bet: unknown): AccountBetDetail | null => {
  const parsed = parseBetPlacement(bet);
  if (!parsed) {
    return null;
  }
  let betRollIndex: number | undefined;
  if (bet && typeof bet === "object") {
    const record = bet as Record<string, unknown>;
    const rawIndex =
      typeof record.bet_roll_index === "number"
        ? record.bet_roll_index
        : typeof record.betRollIndex === "number"
          ? record.betRollIndex
          : undefined;
    if (typeof rawIndex === "number") {
      betRollIndex = rawIndex;
    }
  }

  if (parsed.kind === "chip") {
    return { amount: parsed.amount, kind: "chip", betRollIndex };
  }
  return {
    amount: parsed.amount,
    kind: "strap",
    strap: parsed.strap,
    betRollIndex,
  };
};

const normalizeAccountSnapshot = (input: unknown): AccountSnapshot | null => {
  if (!input || typeof input !== "object") {
    return null;
  }
  const record = input as Record<string, unknown>;
  const snapshot =
    record.snapshot && typeof record.snapshot === "object"
      ? (record.snapshot as Record<string, unknown>)
      : record;
  const strapBets = Array.isArray(snapshot.strap_bets)
    ? snapshot.strap_bets
        .map((entry) => {
          if (!Array.isArray(entry) || entry.length < 2) {
            return null;
          }
          const strap = parseStrap(entry[0]);
          const amount = typeof entry[1] === "number" ? entry[1] : null;
          return strap && amount !== null ? ([strap, amount] as [Strap, number]) : null;
        })
        .filter((entry): entry is [Strap, number] => entry !== null)
    : [];
  const claimedRewards =
    Array.isArray(snapshot.claimed_rewards) && snapshot.claimed_rewards.length >= 2
      ? (() => {
          const chips =
            typeof snapshot.claimed_rewards[0] === "number"
              ? snapshot.claimed_rewards[0]
              : null;
          const straps = Array.isArray(snapshot.claimed_rewards[1])
            ? snapshot.claimed_rewards[1]
                .map((entry) => {
                  if (!Array.isArray(entry) || entry.length < 2) {
                    return null;
                  }
                  const strap = parseStrap(entry[0]);
                  const amount = typeof entry[1] === "number" ? entry[1] : null;
                  return strap && amount !== null
                    ? ([strap, amount] as [Strap, number])
                    : null;
                })
                .filter((entry): entry is [Strap, number] => entry !== null)
            : [];
          return chips === null ? null : ([chips, straps] as [number, [Strap, number][]]);
        })()
      : null;

  return {
    total_chip_bet:
      typeof snapshot.total_chip_bet === "number" ? snapshot.total_chip_bet : 0,
    strap_bets: strapBets,
    total_chip_won:
      typeof snapshot.total_chip_won === "number" ? snapshot.total_chip_won : 0,
    claimed_rewards: claimedRewards,
    per_roll_bets: normalizePerRollBets(snapshot.per_roll_bets),
  };
};

const normalizeStrapMetadata = (input: unknown): StrapMetadata[] => {
  if (!Array.isArray(input)) {
    return [];
  }
  return input
    .map((entry) => {
      if (!entry || typeof entry !== "object") {
        return null;
      }
      const record = entry as Record<string, unknown>;
      const assetId =
        typeof record.asset_id === "string"
          ? normalizeAssetIdValue(record.asset_id)
          : null;
      const strap = parseStrap(record.strap);
      return assetId && strap ? { assetId, strap } : null;
    })
    .filter((entry): entry is StrapMetadata => entry !== null);
};

const normalizeHistoricalSnapshot = (input: unknown): HistoricalSnapshot | null => {
  if (!input || typeof input !== "object") {
    return null;
  }
  const record = input as Record<string, unknown>;
  const snapshot =
    record.snapshot && typeof record.snapshot === "object"
      ? (record.snapshot as Record<string, unknown>)
      : record;
  const rolls = Array.isArray(snapshot.rolls)
    ? snapshot.rolls.map((roll) => roll as Roll)
    : [];
  const modifiers = Array.isArray(snapshot.modifiers)
    ? snapshot.modifiers
        .map((entry) => {
          if (!entry || typeof entry !== "object") {
            return null;
          }
          const modifierRecord = entry as Record<string, unknown>;
          const rollIndex =
            typeof modifierRecord.roll_index === "number"
              ? modifierRecord.roll_index
              : null;
          const modifier =
            typeof modifierRecord.modifier === "string"
              ? modifierRecord.modifier
              : null;
          const modifierRoll =
            typeof modifierRecord.modifier_roll === "string"
              ? (modifierRecord.modifier_roll as Roll)
              : null;
          return rollIndex !== null && modifier && modifierRoll
            ? { rollIndex, modifier, modifierRoll }
            : null;
        })
        .filter(
          (
            entry
          ): entry is { rollIndex: number; modifier: string; modifierRoll: Roll } =>
            entry !== null
        )
    : [];
  const strapRewards = Array.isArray(snapshot.strap_rewards)
    ? snapshot.strap_rewards
        .map((entry) => {
          if (!Array.isArray(entry) || entry.length < 3) {
            return null;
          }
          const roll = entry[0] as Roll;
          const strap = parseStrap(entry[1]);
          const amount = typeof entry[2] === "number" ? entry[2] : null;
          return strap && amount !== null ? ([roll, strap, amount] as [Roll, Strap, number]) : null;
        })
        .filter((entry): entry is [Roll, Strap, number] => entry !== null)
    : [];
  const gameId = typeof snapshot.game_id === "number" ? snapshot.game_id : 0;
  return {
    game_id: gameId,
    rolls,
    modifiers,
    strap_rewards: strapRewards,
  };
};

const rollHitAfterBet = (
  targetRoll: Roll,
  betRollIndex: number,
  rolls: Roll[]
) => rolls.some((roll, index) => roll === targetRoll && betRollIndex <= index);

const rollHitAfterBetWithModifier = (
  targetRoll: Roll,
  betRollIndex: number,
  modifierRollIndex: number,
  rolls: Roll[]
) =>
  rolls.some(
    (roll, index) =>
      roll === targetRoll &&
      betRollIndex <= index &&
      modifierRollIndex <= index
  );

const hasClaimableBets = (
  rolls: Roll[],
  perRollBets: NormalizedRollBets[]
) => {
  if (rolls.length === 0) {
    return false;
  }

  for (const rollEntry of perRollBets) {
    const parsedBets = rollEntry.bets
      .map(parseAccountBetPlacement)
      .filter((bet): bet is AccountBetDetail => bet !== null);
    for (const bet of parsedBets) {
      const betRollIndex = bet.betRollIndex ?? 0;
      if (rollHitAfterBet(rollEntry.roll, betRollIndex, rolls)) {
        return true;
      }
    }
  }

  return false;
};

const normalizeChipAmount = (value: unknown): number | null => {
  if (typeof value === "number") {
    return Number.isFinite(value) ? value : null;
  }
  if (typeof value === "bigint") {
    return Number(value);
  }
  if (value && typeof value === "object") {
    const record = value as { toString?: () => string };
    if (typeof record.toString === "function") {
      const parsed = Number(record.toString());
      return Number.isFinite(parsed) ? parsed : null;
    }
  }
  if (typeof value === "string") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
};

const normalizeIdentityAddress = (value: unknown): string | null => {
  if (!value) {
    return null;
  }
  if (typeof value === "string") {
    return value.toLowerCase();
  }
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    if (typeof record.Address === "string") {
      return record.Address.toLowerCase();
    }
    if (record.Address && typeof record.Address === "object") {
      const addressRecord = record.Address as Record<string, unknown>;
      if (typeof addressRecord.value === "string") {
        return addressRecord.value.toLowerCase();
      }
    }
  }
  return null;
};

const extractClaimLogSummary = (
  logs: unknown[],
  walletAddress: string | null
): { chips: number | null; straps: [Strap, number][] } => {
  const normalizedWallet = walletAddress?.toLowerCase() ?? null;
  for (const log of logs) {
    if (!log || typeof log !== "object") {
      continue;
    }
    const record = log as Record<string, unknown>;
    if (!("total_chips_winnings" in record)) {
      continue;
    }
    if (normalizedWallet) {
      const identity =
        "player" in record
          ? record.player
          : "identity" in record
          ? record.identity
          : null;
      const normalizedIdentity = normalizeIdentityAddress(identity);
      if (normalizedIdentity && normalizedIdentity !== normalizedWallet) {
        continue;
      }
    }
    const chips = normalizeChipAmount(record.total_chips_winnings);
    const straps = Array.isArray(record.total_strap_winnings)
      ? record.total_strap_winnings
          .map((entry) => {
            if (!Array.isArray(entry) || entry.length < 2) {
              return null;
            }
            const strap = parseStrap(entry[0]);
            const amount = normalizeChipAmount(entry[1]);
            return strap && amount !== null ? ([strap, amount] as [Strap, number]) : null;
          })
          .filter((entry): entry is [Strap, number] => entry !== null)
      : [];
    return { chips, straps };
  }
  return { chips: null, straps: [] };
};

const eligibleClaimModifiers = (entry: HistoryEntry) => {
  if (entry.rolls.length === 0) {
    return [];
  }
  const perRollBets = entry.account?.per_roll_bets ?? [];
  if (perRollBets.length === 0) {
    return [];
  }
  const strapBets = perRollBets.flatMap((rollEntry) =>
    rollEntry.bets
      .map((bet) => {
        const parsed = parseAccountBetPlacement(bet);
        if (!parsed || parsed.kind !== "strap") {
          return null;
        }
        return {
          roll: rollEntry.roll,
          betRollIndex: parsed.betRollIndex ?? 0,
        };
      })
      .filter(
        (
          bet
        ): bet is {
          roll: Roll;
          betRollIndex: number;
        } => bet !== null
      )
  );
  if (strapBets.length === 0) {
    return [];
  }
  return entry.modifiers.filter((modifier) =>
    strapBets.some(
      (bet) =>
        bet.roll === modifier.modifierRoll &&
        rollHitAfterBetWithModifier(
          bet.roll,
          bet.betRollIndex,
          modifier.rollIndex,
          entry.rolls
        )
    )
  );
};

export default function App() {
  const baseUrl = useMemo(
    () => normalizeBaseUrl(import.meta.env.VITE_INDEXER_URL as string | undefined),
    []
  );
  const [status, setStatus] = useState<FetchStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [data, setData] = useState<SnapshotResponse | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);
  const [activeRoll, setActiveRoll] = useState<Roll | null>(null);
  const [isGamesOpen, setIsGamesOpen] = useState(false);
  const [isInfoOpen, setIsInfoOpen] = useState(false);
  const [isDiceHistoryOpen, setIsDiceHistoryOpen] = useState(false);
  const [isClosetOpen, setIsClosetOpen] = useState(false);
  const [betTargetRoll, setBetTargetRoll] = useState<Roll | null>(null);
  const [betKind, setBetKind] = useState<"chip" | "strap">("chip");
  const [betAmount, setBetAmount] = useState("1");
  const [betStrapKind, setBetStrapKind] = useState<string | null>(null);
  const [betStrapAssetId, setBetStrapAssetId] = useState<string | null>(null);
  const [isStrapKindPickerOpen, setIsStrapKindPickerOpen] = useState(false);
  const [isStrapPickerOpen, setIsStrapPickerOpen] = useState(false);
  const [betStatus, setBetStatus] = useState<
    "idle" | "signing" | "pending" | "success" | "error"
  >("idle");
  const [betError, setBetError] = useState<string | null>(null);
  const [betTxId, setBetTxId] = useState<string | null>(null);
  const [modifierPurchaseKey, setModifierPurchaseKey] = useState<string | null>(
    null
  );
  const [modifierPurchaseStatus, setModifierPurchaseStatus] = useState<
    "idle" | "signing" | "pending" | "success" | "error"
  >("idle");
  const [modifierPurchaseError, setModifierPurchaseError] = useState<string | null>(
    null
  );
  const [claimStatus, setClaimStatus] = useState<
    "idle" | "signing" | "pending" | "success" | "error"
  >("idle");
  const [claimError, setClaimError] = useState<string | null>(null);
  const [claimGameId, setClaimGameId] = useState<number | null>(null);
  const [claimResult, setClaimResult] = useState<ClaimResult | null>(null);
  const [claimModifierEntry, setClaimModifierEntry] = useState<HistoryEntry | null>(
    null
  );
  const [claimModifierOptions, setClaimModifierOptions] = useState<
    HistoryEntry["modifiers"]
  >([]);
  const [claimModifierSelection, setClaimModifierSelection] = useState<string[]>(
    []
  );
  const [isRollAnimating, setIsRollAnimating] = useState(false);
  const [rollingFace, setRollingFace] = useState<Roll | null>(null);
  const [rollLandPulse, setRollLandPulse] = useState(false);
  const [rollFallbackFace, setRollFallbackFace] = useState<Roll | null>("Seven");
  const [chipBalanceOverride, setChipBalanceOverride] = useState<string | null>(
    null
  );
  const rollAnimationRef = useRef<number | null>(null);
  const rollAnimationEndRef = useRef<number>(0);
  const rollAnimationOriginRef = useRef<Roll | null>(null);
  const rollAnimationOriginCountRef = useRef<number>(0);
  const [expandedOrigin, setExpandedOrigin] = useState<{
    roll: Roll;
    originLeft: number;
    originTop: number;
    originWidth: number;
    originHeight: number;
    targetWidth: number;
    targetHeight: number;
    translateX: number;
    translateY: number;
  } | null>(null);
  const [isExpandedActive, setIsExpandedActive] = useState(false);
  const [networkKey, setNetworkKey] =
    useState<FuelNetworkKey>(DEFAULT_NETWORK);
  const [walletError, setWalletError] = useState<string | null>(null);
  const { connect, isConnecting } = useConnectUI();
  const { isConnected } = useIsConnected();
  const { wallet } = useWallet();
  const { account } = useAccount();
  const { provider } = useProvider();
  const { selectNetworkAsync } = useSelectNetwork();
  const [baseAssetId, setBaseAssetId] = useState<string | null>(null);
  const [accountSnapshot, setAccountSnapshot] = useState<AccountSnapshot | null>(
    null
  );
  const [knownStraps, setKnownStraps] = useState<StrapMetadata[]>([]);
  const [ownedStraps, setOwnedStraps] = useState<OwnedStrap[]>([]);
  const [gameHistory, setGameHistory] = useState<HistoryEntry[]>([]);
  const snapshot = data?.snapshot ?? null;
  const walletStatus: "idle" | "connecting" | "connected" | "error" = walletError
    ? "error"
    : isConnecting
      ? "connecting"
      : isConnected
        ? "connected"
        : "idle";
  const walletAddress = account ?? null;
  const chipAssetId = FUEL_NETWORKS[networkKey].chipAssetId;
  const chipAssetTicker = FUEL_NETWORKS[networkKey].chipAssetTicker;
  const baseAssetTicker = FUEL_NETWORKS[networkKey].baseAssetTicker;
  const { balance: chipBalance } = useBalance({
    account: walletAddress,
    assetId: chipAssetId,
  });
  const displayChipBalance = chipBalanceOverride ?? chipBalance;
  const closetGroups = useMemo(() => {
    if (ownedStraps.length === 0) {
      return [];
    }

    const modifierRank = new Map(
      modifierOrder.map((modifier, index) => [modifier, index])
    );
    const grouped = new Map<string, OwnedStrap[]>();
    ownedStraps.forEach((entry) => {
      const kind = entry.strap.kind;
      const existing = grouped.get(kind);
      if (existing) {
        existing.push(entry);
      } else {
        grouped.set(kind, [entry]);
      }
    });

    const sortedKinds = [
      ...strapKindOrder.filter((kind) => grouped.has(kind)),
      ...Array.from(grouped.keys())
        .filter((kind) => !strapKindOrder.includes(kind))
        .sort(),
    ];

    return sortedKinds.map((kind) => {
      const entries = [...(grouped.get(kind) ?? [])].sort((a, b) => {
        const rankA = modifierRank.get(a.strap.modifier) ?? Number.MAX_SAFE_INTEGER;
        const rankB = modifierRank.get(b.strap.modifier) ?? Number.MAX_SAFE_INTEGER;
        if (rankA !== rankB) {
          return rankA - rankB;
        }
        if (a.strap.level !== b.strap.level) {
          return a.strap.level - b.strap.level;
        }
        return a.assetId.localeCompare(b.assetId);
      });

      return {
        kind,
        emoji: strapEmojis[kind] ?? "🎁",
        entries,
      };
    });
  }, [ownedStraps]);

  useEffect(() => {
    if (isConnected) {
      setWalletError(null);
    }
  }, [isConnected]);

  useEffect(() => {
    if (betKind !== "strap") {
      return;
    }
    if (!betStrapKind || !closetGroups.some((group) => group.kind === betStrapKind)) {
      setBetStrapKind(closetGroups[0]?.kind ?? null);
    }
  }, [betKind, betStrapKind, closetGroups]);

  useEffect(() => {
    if (betKind !== "strap") {
      return;
    }
    if (!betStrapKind) {
      setBetStrapAssetId(null);
      return;
    }
    const group = closetGroups.find((entry) => entry.kind === betStrapKind);
    const belongsToKind = ownedStraps.some(
      (strap) => strap.assetId === betStrapAssetId && strap.strap.kind === betStrapKind
    );
    if (!belongsToKind) {
      setBetStrapAssetId(group?.entries[0]?.assetId ?? null);
    }
  }, [betKind, betStrapKind, betStrapAssetId, ownedStraps, closetGroups]);

  useEffect(() => {
    if (!provider || !isConnected) {
      setBaseAssetId(null);
      return;
    }

    let cancelled = false;
    provider
      .getBaseAssetId()
      .then((id) => {
        if (!cancelled) {
          setBaseAssetId(id);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setBaseAssetId(null);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [provider, isConnected]);

  useEffect(() => {
    if (!isConnected) {
      return;
    }

    let cancelled = false;
    const selectNetwork = async () => {
      try {
        await selectNetworkAsync({ url: FUEL_NETWORKS[networkKey].graphqlUrl });
      } catch (err) {
        if (cancelled) {
          return;
        }
        const message =
          err instanceof Error ? err.message : "Wallet network error";
        setWalletError(message);
      }
    };

    selectNetwork();

    return () => {
      cancelled = true;
    };
  }, [isConnected, networkKey, selectNetworkAsync]);

  useEffect(() => {
    if (!baseUrl) {
      setKnownStraps([]);
      return;
    }

    let cancelled = false;
    const loadStraps = async () => {
      try {
        const response = await fetch(`${baseUrl}/straps`);
        if (!response.ok) {
          throw new Error(`strap metadata responded with ${response.status}`);
        }
        const payload = await response.json();
        const straps = normalizeStrapMetadata(payload);
        if (!cancelled) {
          setKnownStraps(straps);
        }
      } catch (err) {
        if (!cancelled) {
          setKnownStraps([]);
        }
      }
    };

    loadStraps();

    return () => {
      cancelled = true;
    };
  }, [baseUrl]);

  useEffect(() => {
    if (!baseUrl || !walletAddress) {
      setAccountSnapshot(null);
      return;
    }

    let cancelled = false;
    let timeoutId: number | undefined;

    const loadAccountSnapshot = async () => {
      if (cancelled) {
        return;
      }

      try {
        const response = await fetch(`${baseUrl}/account/${walletAddress}`);
        if (response.status === 404) {
          if (!cancelled) {
            setAccountSnapshot(null);
          }
        } else if (!response.ok) {
          throw new Error(`account snapshot responded with ${response.status}`);
        } else {
          const payload = (await response.json()) as AccountSnapshotResponse | null;
          const snapshot = payload ? normalizeAccountSnapshot(payload) : null;
          if (!cancelled) {
            setAccountSnapshot(snapshot);
          }
        }
      } catch (err) {
        if (!cancelled) {
          setAccountSnapshot(null);
        }
      } finally {
        if (!cancelled) {
          timeoutId = window.setTimeout(loadAccountSnapshot, POLL_INTERVAL_MS);
        }
      }
    };

    loadAccountSnapshot();

    return () => {
      cancelled = true;
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [baseUrl, walletAddress]);

  const refreshBalances = async () => {
    if (!provider || !walletAddress) {
      setOwnedStraps([]);
      setChipBalanceOverride(null);
      return;
    }
    try {
      const result = await provider.getBalances(walletAddress);
      const balances = Array.isArray(result)
        ? result
        : (result as { balances?: Array<{ assetId: string; amount: unknown }> })
            .balances ?? [];
      const byAssetId = new Map<string, string>();
      balances.forEach((entry) => {
        const assetId =
          typeof entry.assetId === "string" ? entry.assetId : String(entry.assetId);
        const normalizedAssetId = normalizeAssetIdValue(assetId);
        if (!normalizedAssetId) {
          return;
        }
        const amount =
          typeof entry.amount === "string"
            ? entry.amount
            : entry.amount?.toString?.() ?? String(entry.amount);
        byAssetId.set(normalizedAssetId, amount);
      });
      const owned = knownStraps
        .map((strap) => {
          const amount = byAssetId.get(strap.assetId);
          if (!amount) {
            return null;
          }
          try {
            if (BigInt(amount) <= 0n) {
              return null;
            }
          } catch (err) {
            return null;
          }
          return {
            assetId: strap.assetId,
            strap: strap.strap,
            amount,
          };
        })
        .filter((entry): entry is OwnedStrap => entry !== null);

      setOwnedStraps(owned);
      setChipBalanceOverride(byAssetId.get(chipAssetId) ?? null);
    } catch (err) {
      setOwnedStraps([]);
    }
  };

  useEffect(() => {
    if (!provider || !walletAddress) {
      setOwnedStraps([]);
      setChipBalanceOverride(null);
      return;
    }

    let cancelled = false;
    const runRefresh = async () => {
      if (cancelled) {
        return;
      }
      await refreshBalances();
    };

    runRefresh();
    const intervalId = window.setInterval(runRefresh, 10000);

    return () => {
      cancelled = true;
      window.clearInterval(intervalId);
    };
  }, [provider, walletAddress, knownStraps, chipAssetId]);

  useEffect(() => {
    if (!baseUrl || !walletAddress || !snapshot) {
      setGameHistory([]);
      return;
    }

    const currentGameId = snapshot.game_id;
    if (currentGameId === 0) {
      setGameHistory([]);
      return;
    }

    const historyDepth = 8;
    const gameIds = Array.from(
      { length: Math.min(currentGameId, historyDepth) },
      (_, index) => currentGameId - 1 - index
    );
    let cancelled = false;
    const loadHistory = async () => {
      const entries = await Promise.all(
        gameIds.map(async (gameId) => {
          try {
            const [historyResponse, accountResponse] = await Promise.all([
              fetch(`${baseUrl}/historical/${gameId}`),
              fetch(`${baseUrl}/account/${walletAddress}/${gameId}`),
            ]);
            if (!historyResponse.ok) {
              return null;
            }
            const historyPayload = await historyResponse.json();
            const history = normalizeHistoricalSnapshot(historyPayload);
            if (!history) {
              return null;
            }
            let account: AccountSnapshot | null = null;
            if (accountResponse.ok) {
              const accountPayload = await accountResponse.json();
              account = normalizeAccountSnapshot(accountPayload);
            }
            const claimed = Boolean(account?.claimed_rewards);
            return {
              gameId: history.game_id,
              rolls: history.rolls,
              modifiers: history.modifiers,
              strapRewards: history.strap_rewards,
              account,
              claimed,
            } as HistoryEntry;
          } catch (err) {
            return null;
          }
        })
      );

      if (!cancelled) {
        setGameHistory(
          entries.filter((entry): entry is HistoryEntry => entry !== null)
        );
      }
    };

    loadHistory();

    return () => {
      cancelled = true;
    };
  }, [baseUrl, walletAddress, snapshot]);
  const [rollStatus, setRollStatus] = useState<
    "idle" | "signing" | "pending" | "success" | "error"
  >("idle");
  const [rollError, setRollError] = useState<string | null>(null);
  const [rollTxId, setRollTxId] = useState<string | null>(null);

  useEffect(() => {
    if (!activeRoll) {
      setExpandedOrigin(null);
      setIsExpandedActive(false);
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setActiveRoll(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [activeRoll]);

  const openExpandedShop = (roll: Roll, target: HTMLElement) => {
    const rect = target.getBoundingClientRect();
    const targetWidth = Math.min(900, window.innerWidth * 0.92);
    const targetHeight = Math.min(520, window.innerHeight * 0.74);
    const targetLeft = (window.innerWidth - targetWidth) / 2;
    const targetTop = (window.innerHeight - targetHeight) / 2;

    setExpandedOrigin({
      roll,
      originLeft: rect.left,
      originTop: rect.top,
      originWidth: rect.width,
      originHeight: rect.height,
      targetWidth,
      targetHeight,
      translateX: targetLeft - rect.left,
      translateY: targetTop - rect.top,
    });
    setIsExpandedActive(false);
    requestAnimationFrame(() => {
      setActiveRoll(roll);
      requestAnimationFrame(() => {
        setIsExpandedActive(true);
      });
    });
  };
  const openBetModal = (roll: Roll) => {
    setBetTargetRoll(roll);
    setBetKind("chip");
    setBetAmount("1");
    setBetStrapKind(closetGroups[0]?.kind ?? null);
    setBetStrapAssetId(closetGroups[0]?.entries[0]?.assetId ?? null);
    setIsStrapKindPickerOpen(false);
    setIsStrapPickerOpen(false);
    setBetError(null);
    setBetTxId(null);
    setBetStatus("idle");
  };
  const closeBetModal = () => {
    setBetTargetRoll(null);
    setIsStrapKindPickerOpen(false);
    setIsStrapPickerOpen(false);
    setBetError(null);
    setBetTxId(null);
    setBetStatus("idle");
  };
  const closeClaimModifier = () => {
    setClaimModifierEntry(null);
    setClaimModifierOptions([]);
    setClaimModifierSelection([]);
  };
  const closeClaimResult = () => {
    setClaimResult(null);
    setClaimError(null);
    setClaimStatus("idle");
    setClaimGameId(null);
  };
  const isAnyModalOpen = Boolean(
    activeRoll ||
      isGamesOpen ||
      isInfoOpen ||
      isDiceHistoryOpen ||
      isClosetOpen ||
      betTargetRoll ||
      isStrapKindPickerOpen ||
      isStrapPickerOpen ||
      Boolean(claimModifierEntry) ||
      Boolean(claimResult)
  );

  useEffect(() => {
    if (!isAnyModalOpen) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }

      event.preventDefault();
      if (claimModifierEntry) {
        setClaimModifierEntry(null);
        setClaimModifierSelection([]);
        return;
      }
      if (claimResult) {
        closeClaimResult();
        return;
      }
      if (isStrapPickerOpen) {
        setIsStrapPickerOpen(false);
        return;
      }
      if (isStrapKindPickerOpen) {
        setIsStrapKindPickerOpen(false);
        return;
      }
      if (betTargetRoll) {
        closeBetModal();
        return;
      }
      if (isClosetOpen) {
        setIsClosetOpen(false);
        return;
      }
      if (isDiceHistoryOpen) {
        setIsDiceHistoryOpen(false);
        return;
      }
      if (isInfoOpen) {
        setIsInfoOpen(false);
        return;
      }
      if (isGamesOpen) {
        setIsGamesOpen(false);
        return;
      }
      if (activeRoll) {
        setActiveRoll(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [
    isAnyModalOpen,
    isStrapPickerOpen,
    isStrapKindPickerOpen,
    betTargetRoll,
    isClosetOpen,
    isDiceHistoryOpen,
    isInfoOpen,
    isGamesOpen,
    activeRoll,
    claimModifierEntry,
    claimResult,
  ]);

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

  const rewardsByRoll = useMemo(() => {
    const map = new Map<Roll, [Strap, number][]>();
    if (!snapshot) {
      return map;
    }
    for (const [roll, strap, amount] of snapshot.rewards) {
      const existing = map.get(roll) ?? [];
      existing.push([strap, amount]);
      map.set(roll, existing);
    }
    return map;
  }, [snapshot]);

  const shopEntries = useMemo(
    () => normalizeModifierShop(snapshot?.modifier_shop),
    [snapshot]
  );

  const modifierShopByRoll = useMemo(
    () =>
      shopEntries.reduce((map, entry) => {
        const roll = entry.modifier_roll as Roll;
        const existing = map.get(roll) ?? [];
        existing.push(entry);
        map.set(roll, existing);
        return map;
      }, new Map<Roll, ModifierShopEntry[]>()),
    [shopEntries]
  );

  const formatRewardCompact = (strap: Strap) => {
    const modifierEmoji = modifierEmojis[strap.modifier] ?? "";
    const strapEmoji = strapEmojis[strap.kind] ?? "🎽";
    const level = strap.level;
    return `${modifierEmoji}${strapEmoji}${level}`;
  };

  const formatNumber = (value: number | null | undefined) =>
    value === null || value === undefined ? "—" : value.toLocaleString();

  const formatAddress = (address: string | null) => {
    if (!address) {
      return "Not connected";
    }
    const trimmed = address.toLowerCase();
    return `${trimmed.slice(0, 6)}...${trimmed.slice(-4)}`;
  };

  const diceRolls = useMemo(() => {
    if (!snapshot || snapshot.rolls.length === 0) {
      return [];
    }
    return snapshot.rolls.slice(-6).reverse();
  }, [snapshot]);
  const lastRoll = diceRolls[0] ?? null;
  const rollCount = snapshot?.rolls.length ?? 0;
  const displayedRoll =
    isRollAnimating && rollingFace ? rollingFace : lastRoll ?? rollFallbackFace;

  const getRollIndex = (roll: Roll) => rollOrder.indexOf(roll);

  const buildShopClasses = (options: {
    hasReward: boolean;
    hasTableBets: boolean;
    modifier: string | null;
    index: number;
  }) => {
    const base = ["shop-tile", `shop-tile--${options.index % 5}`];
    if (options.hasReward) {
      base.push("shop-tile--reward");
    }
    if (options.hasTableBets) {
      base.push("shop-tile--table");
    }
    if (options.modifier) {
      const modifierClass = modifierClassNames[options.modifier];
      if (modifierClass) {
        base.push(`shop-tile--${modifierClass}`);
      }
    }
    return base.join(" ");
  };

  const connectWallet = async () => {
    setWalletError(null);

    try {
      if (isConnected) {
        return true;
      }
      await connect();
      return true;
    } catch (err) {
      const message = err instanceof Error ? err.message : "Wallet error";
      setWalletError(message);
      return false;
    }
  };

  const handleRoll = async () => {
    setRollError(null);
    setRollTxId(null);

    if (!isConnected || !wallet) {
      setWalletError("Connect wallet to roll");
      setRollStatus("error");
      return;
    }

    setRollStatus("signing");
    setIsRollAnimating(true);
    rollAnimationOriginRef.current = lastRoll;
    rollAnimationOriginCountRef.current = rollCount;
    rollAnimationEndRef.current = Date.now() + 1000;
    setRollFallbackFace(null);
    if (rollAnimationRef.current !== null) {
      window.clearInterval(rollAnimationRef.current);
    }
    rollAnimationRef.current = window.setInterval(() => {
      setRollingFace((prev) => {
        const currentIndex = prev ? rollOrder.indexOf(prev) : -1;
        const nextIndex = (currentIndex + 1) % rollOrder.length;
        return rollOrder[nextIndex] ?? rollOrder[0];
      });
    }, 90);

    try {
      const contract = createStrappedContract(wallet, networkKey);
      const response = await contract.functions.roll_dice().call();
      setRollTxId(response.transactionId);
      setRollStatus("pending");
      await response.waitForResult();
      setRollStatus("success");
      await refreshBalances();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Roll failed";
      setRollError(message);
      setRollStatus("error");
    }
  };

  const handlePlaceBet = async () => {
    setBetError(null);
    setBetTxId(null);

    if (!betTargetRoll) {
      return;
    }
    if (!isConnected || !wallet) {
      setBetError("Connect wallet to bet");
      setBetStatus("error");
      return;
    }

    const amount = Number(betAmount);
    if (!Number.isFinite(amount) || amount <= 0) {
      setBetError("Enter a valid amount");
      setBetStatus("error");
      return;
    }

    const strapSelection =
      betKind === "strap"
        ? ownedStraps.find((entry) => entry.assetId === betStrapAssetId) ?? null
        : null;
    if (betKind === "strap" && !strapSelection) {
      setBetError("Select a strap to bet");
      setBetStatus("error");
      return;
    }

    setBetStatus("signing");

    try {
      const contract = createStrappedContract(wallet, networkKey);
      const bet =
        betKind === "chip"
          ? { Chip: undefined }
          : { Strap: strapSelection?.strap };
      const assetId =
        betKind === "chip" ? chipAssetId : strapSelection?.assetId ?? null;
      if (!assetId) {
        setBetError("Missing asset id for bet");
        setBetStatus("error");
        return;
      }
      const response = await contract.functions
        .place_bet(betTargetRoll, bet, amount)
        .callParams({
          forward: {
            amount,
            assetId,
          },
        })
        .call();
      setBetTxId(response.transactionId);
      setBetStatus("pending");
      await response.waitForResult();
      setBetStatus("success");
      await refreshBalances();
    } catch (err) {
      const message = err instanceof Error ? err.message : "Bet failed";
      setBetError(message);
      setBetStatus("error");
    }
  };

  const handlePurchaseModifier = async (
    roll: Roll,
    entry: ModifierShopEntry,
    entryKey: string
  ) => {
    setModifierPurchaseError(null);

    if (!isConnected || !wallet) {
      setModifierPurchaseError("Connect wallet to purchase");
      setModifierPurchaseStatus("error");
      setModifierPurchaseKey(entryKey);
      return;
    }

    setModifierPurchaseKey(entryKey);
    setModifierPurchaseStatus("signing");

    try {
      const contract = createStrappedContract(wallet, networkKey);
      const response = await contract.functions
        .purchase_modifier(roll, entry.modifier)
        .callParams({
          forward: {
            amount: entry.price,
            assetId: chipAssetId,
          },
        })
        .call();
      setModifierPurchaseStatus("pending");
      await response.waitForResult();
      setModifierPurchaseStatus("success");
      await refreshBalances();
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Purchase modifier failed";
      setModifierPurchaseError(message);
      setModifierPurchaseStatus("error");
    }
  };

  const handleClaimRewards = async (
    entry: HistoryEntry,
    enabledModifiers: Array<[Roll, string]>
  ) => {
    setClaimError(null);
    setClaimGameId(entry.gameId);

    if (!isConnected || !wallet) {
      setWalletError("Connect wallet to claim");
      setClaimError("Connect wallet to claim");
      setClaimStatus("error");
      return;
    }

    setClaimStatus("signing");

    try {
      let preChipBalance: bigint | null = null;
      try {
        const balance = await wallet.getBalance(chipAssetId);
        preChipBalance = BigInt(balance.toString());
      } catch (err) {
        preChipBalance = null;
      }

      const contract = createStrappedContract(wallet, networkKey);
      const response = await contract.functions
        .claim_rewards(entry.gameId, enabledModifiers)
        .call();
      setClaimStatus("pending");
      const claimResultValue = await response.waitForResult();
      setClaimStatus("success");
      await refreshBalances();

      let chipDelta: number | null = null;
      try {
        const postBalance = await wallet.getBalance(chipAssetId);
        if (preChipBalance !== null) {
          const postValue = BigInt(postBalance.toString());
          const delta = postValue > preChipBalance ? postValue - preChipBalance : 0n;
          chipDelta = Number(delta);
        }
      } catch (err) {
        chipDelta = null;
      }

      const accountBets = entry.account?.per_roll_bets ?? [];
      const accountBetPlacements = accountBets.flatMap((rollEntry) =>
        rollEntry.bets
          .map((bet) => {
            const parsed = parseAccountBetPlacement(bet);
            if (!parsed) {
              return null;
            }
            return {
              roll: rollEntry.roll,
              kind: parsed.kind,
              amount: parsed.amount,
              strap: parsed.kind === "strap" ? parsed.strap : undefined,
              betRollIndex: parsed.betRollIndex,
            };
          })
          .filter(
            (
              bet
            ): bet is {
              roll: Roll;
              kind: "chip" | "strap";
              amount: number;
              strap: Strap | undefined;
              betRollIndex: number | undefined;
            } => bet !== null
          )
      );
      const accountBetsSummary = summarizeBetsByKind(
        accountBets.flatMap((bets) => bets.bets)
      );
      const claimLogs =
        claimResultValue && "logs" in claimResultValue
          ? (claimResultValue as { logs?: unknown[] }).logs ?? []
          : [];
      const claimLogSummary = extractClaimLogSummary(claimLogs, walletAddress);
      const chipWon = claimLogSummary.chips ?? chipDelta ?? null;
      const chipBet = accountBetsSummary.chipTotal;
      const netChip =
        chipWon !== null && chipBet !== null ? chipWon - chipBet : null;
      const straps =
        claimLogSummary.straps.length > 0
          ? claimLogSummary.straps
          : entry.account?.claimed_rewards?.[1] ?? [];

      setClaimResult({
        gameId: entry.gameId,
        chipWon,
        chipBet,
        netChip,
        betDetails: accountBetPlacements,
        straps,
        txId: response.transactionId,
      });

      setGameHistory((prev) =>
        prev.map((item) =>
          item.gameId === entry.gameId ? { ...item, claimed: true } : item
        )
      );
    } catch (err) {
      const message = err instanceof Error ? err.message : "Claim failed";
      setClaimError(message);
      setClaimStatus("error");
    }
  };

  const isRolling = rollStatus === "signing" || rollStatus === "pending";
  const rollButtonLabel = (() => {
    if (!isConnected) {
      return "Connect wallet to play";
    }
    if (rollStatus === "signing") {
      return "Signing...";
    }
    if (rollStatus === "pending") {
      return "Rolling...";
    }
    if (rollStatus === "success") {
      return "Roll again";
    }
    return "Roll";
  })();
  const rollStatusMessage = (() => {
    if (walletError) {
      return `Wallet: ${walletError}`;
    }
    if (rollError) {
      return `Roll: ${rollError}`;
    }
    return isConnected ? null : "Connect wallet to play.";
  })();
  const betStatusMessage = (() => {
    if (betError) {
      return betError;
    }
    if (betStatus === "signing") {
      return "Signing bet...";
    }
    if (betStatus === "pending") {
      return "Bet pending...";
    }
    if (betStatus === "success") {
      return "Bet placed.";
    }
    return null;
  })();
  const isBetBusy = betStatus === "signing" || betStatus === "pending";
  const isClaimBusy = claimStatus === "signing" || claimStatus === "pending";
  const selectedBetStrap = ownedStraps.find(
    (entry) => entry.assetId === betStrapAssetId
  );
  const selectedBetGroup = closetGroups.find(
    (group) => group.kind === betStrapKind
  );

  useEffect(() => {
    if (!isRollAnimating) {
      return;
    }
    const origin = rollAnimationOriginRef.current;
    const originCount = rollAnimationOriginCountRef.current;
    if (Date.now() < rollAnimationEndRef.current) {
      return;
    }
    const hasNewRoll = rollCount > originCount || (origin && lastRoll && origin !== lastRoll);
    if (hasNewRoll) {
      setIsRollAnimating(false);
      setRollingFace(null);
      rollAnimationOriginRef.current = null;
      rollAnimationOriginCountRef.current = rollCount;
      if (rollAnimationRef.current !== null) {
        window.clearInterval(rollAnimationRef.current);
        rollAnimationRef.current = null;
      }
      setRollLandPulse(true);
      return;
    }
    if (!lastRoll && rollCount === 0) {
      setIsRollAnimating(false);
      setRollingFace(null);
      rollAnimationOriginRef.current = null;
      rollAnimationOriginCountRef.current = rollCount;
      if (rollAnimationRef.current !== null) {
        window.clearInterval(rollAnimationRef.current);
        rollAnimationRef.current = null;
      }
      setRollFallbackFace("Seven");
      setRollLandPulse(true);
    }
  }, [isRollAnimating, lastRoll, rollCount]);

  useEffect(() => {
    if (rollStatus !== "error") {
      return;
    }
    setIsRollAnimating(false);
    setRollingFace(null);
    rollAnimationOriginRef.current = null;
    if (rollAnimationRef.current !== null) {
      window.clearInterval(rollAnimationRef.current);
      rollAnimationRef.current = null;
    }
  }, [rollStatus]);

  useEffect(() => {
    if (lastRoll) {
      setRollFallbackFace(null);
    }
  }, [lastRoll]);

  useEffect(() => {
    if (!rollLandPulse) {
      return;
    }
    const timeoutId = window.setTimeout(() => {
      setRollLandPulse(false);
    }, 450);
    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [rollLandPulse]);

  return (
    <div className={`street-app${activeRoll ? " street-app--expanded" : ""}`}>
      <header className="street-header">
        <h1 className="street-title">STRAPPED!</h1>
        <div className="street-meta">
          <span className={`status-chip status-chip--${status}`}>{status}</span>
          <div className="wallet-pill">
            <span>{formatAddress(walletAddress)}</span>
            {walletStatus === "connected" ? null : (
              <span className="wallet-pill__dot" />
            )}
          </div>
          <label className="network-picker" htmlFor="network-select">
            <span>Network</span>
            <select
              id="network-select"
              className="network-picker__select"
              value={networkKey}
              onChange={(event) =>
                setNetworkKey(event.target.value as FuelNetworkKey)
              }
              disabled={walletStatus === "connecting"}
            >
              {Object.entries(FUEL_NETWORKS).map(([key, network]) => (
                <option key={key} value={key}>
                  {network.label}
                </option>
              ))}
            </select>
          </label>
          <button
            className="ghost-button"
            type="button"
            onClick={connectWallet}
            disabled={walletStatus === "connecting"}
          >
            {walletStatus === "connecting" ? "Connecting..." : "Connect"}
          </button>
        </div>
      </header>

      <main className="street-stage">
        <div className="sky-glow" />
        <div className="street-sun" />
        <div className="cloud cloud--one" />
        <div className="cloud cloud--two" />
        <div className="street-ground" />

        <div className="roll-panel">
          <div className="roll-panel__row roll-panel__row--main">
            <div className="roll-panel__last">
              <span className="roll-panel__label">Last roll</span>
              {displayedRoll ? (
                <div
                  className={`dice-card dice-card--single${
                    rollLandPulse ? " dice-card--land" : ""
                  }`}
                >
                  <div className="dice-face">{rollNumbers[displayedRoll]}</div>
                  <div className="dice-label">
                    {lastRoll ? rollLabels[displayedRoll] : "NEW GAME :)"}
                  </div>
                </div>
              ) : null}
            </div>
            <div className="roll-panel__actions">
              <button
                className="primary-button roll-button"
                type="button"
                onClick={handleRoll}
                disabled={!isConnected || isRolling || walletStatus === "connecting"}
              >
                {rollButtonLabel}
              </button>
              <button
                className="ghost-button roll-history-button"
                type="button"
                onClick={() => setIsDiceHistoryOpen(true)}
                disabled={diceRolls.length === 0}
              >
                History
              </button>
            </div>
          </div>
          {rollStatusMessage ? (
            <div className="roll-panel__status">{rollStatusMessage}</div>
          ) : null}
        </div>

        <section className="shops-row">
          {rollOrder.map((roll, index) => {
            const rollBets = snapshot?.specific_bets?.[index];
            const totalChips = rollBets ? rollBets[0] : null;
            const strapBets = rollBets ? rollBets[1] : [];
            const rewards = rewardsByRoll.get(roll) ?? [];
            const modifier = snapshot?.modifiers_active?.[index] ?? null;
            const modifierStory = modifier ? modifierStories[modifier] ?? null : null;
            const modifierEntries = modifierShopByRoll.get(roll) ?? [];
            const visibleEntries = modifierEntries.filter((entry) => !entry.purchased);
            const modifierEntry = modifier
              ? modifierEntries.find((entry) => entry.modifier === modifier)
              : null;
            const shouldShowModifierBanner = Boolean(modifier) &&
              (modifierEntry ? Boolean(modifierEntry.purchased) : true);
            const hasTableBets = Boolean(
              (totalChips ?? 0) > 0 || strapBets.length > 0
            );
            const totalStrapBets = strapBets.reduce(
              (sum, [, amount]) => sum + amount,
              0
            );
            const accountRollEntry = accountSnapshot?.per_roll_bets.find(
              (entry) => entry.roll === roll
            );
            const accountBetDetails = (accountRollEntry?.bets ?? [])
              .map(parseAccountBetPlacement)
              .filter((entry): entry is AccountBetDetail => entry !== null);
            const hasAccountBets = accountBetDetails.length > 0;
            const shopClassName = buildShopClasses({
              hasReward: rewards.length > 0,
              hasTableBets,
              modifier,
              index,
            });
            const isExpanded = activeRoll === roll;
            const tableBetsForRoll = (snapshot?.table_bets ?? [])
              .map((entry) => {
                const perRoll = normalizePerRollBets(entry.per_roll_bets);
                const rollEntry = perRoll.find(
                  (rollEntry) => rollEntry.roll === roll
                );
                if (!rollEntry) {
                  return null;
                }
                const summary = summarizeBetsByKind(rollEntry.bets);
                if (summary.chipTotal === 0 && summary.straps.length === 0) {
                  return null;
                }
                return {
                  identity: entry.identity,
                  chipTotal: summary.chipTotal,
                  straps: summary.straps,
                };
              })
              .filter(
                (
                  entry
                ): entry is {
                  identity: unknown;
                  chipTotal: number;
                  straps: [Strap, number][];
                } => entry !== null
              );
            const tableStrapSummary = tableBetsForRoll.reduce(
              (summary, entry) => {
                entry.straps.forEach(([strap, amount]) => {
                  const key = strapKey(strap);
                  const existing = summary.get(key);
                  summary.set(key, {
                    strap,
                    amount: (existing?.amount ?? 0) + amount,
                  });
                });
                return summary;
              },
              new Map<string, { strap: Strap; amount: number }>()
            );
            const tableStrapTotals = Array.from(tableStrapSummary.values());
            const expandedStyle =
              expandedOrigin?.roll === roll
                ? ({
                    "--origin-left": `${expandedOrigin.originLeft}px`,
                    "--origin-top": `${expandedOrigin.originTop}px`,
                    "--origin-width": `${expandedOrigin.originWidth}px`,
                    "--origin-height": `${expandedOrigin.originHeight}px`,
                    "--target-width": `${expandedOrigin.targetWidth}px`,
                    "--target-height": `${expandedOrigin.targetHeight}px`,
                    "--translate-x": `${expandedOrigin.translateX}px`,
                    "--translate-y": `${expandedOrigin.translateY}px`,
                  } as React.CSSProperties)
                : undefined;
            const handleShopKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
              if (event.key !== "Enter" && event.key !== " ") {
                return;
              }
              event.preventDefault();
              openExpandedShop(roll, event.currentTarget);
            };

            return (
              <div key={roll} className="shop-cell">
                <div
                  className={`shop-frame${
                    isExpanded ? " shop-frame--expanded" : ""
                  }`}
                >
                  <div
                    className={`${shopClassName}${isExpanded ? " shop-tile--expanded" : ""}${
                      isExpanded && isExpandedActive
                        ? " shop-tile--expanded-active"
                        : ""
                    }`}
                    role="button"
                    tabIndex={0}
                    aria-expanded={isExpanded}
                    onClick={(event) => openExpandedShop(roll, event.currentTarget)}
                    onKeyDown={handleShopKeyDown}
                    style={expandedStyle}
                  >
                    <div className="shop-sign">
                      <span className="shop-sign__label">{rollLabels[roll]}</span>
                    </div>
                    <div className="shop-awning" />
                    <div className="shop-facade">
                      <div className="shop-window">
                        {!isExpanded ? (
                          <div className="shop-meta">
                            <div className="shop-meta__section">
                              <span className="shop-meta__title">Your bets</span>
                              {hasAccountBets ? (
                                <div className="shop-meta__stack">
                                  {accountBetDetails.map((detail, detailIndex) => (
                                    <span key={`bet-${roll}-${detailIndex}`}>
                                      {detail.kind === "chip"
                                        ? `Chip x${formatNumber(detail.amount)}`
                                        : `${formatRewardCompact(detail.strap)} x${formatNumber(
                                            detail.amount
                                          )}`}
                                      {typeof detail.betRollIndex === "number"
                                        ? ` @${detail.betRollIndex}`
                                        : ""}
                                    </span>
                                  ))}
                                </div>
                              ) : (
                                <span className="shop-meta__muted">None yet.</span>
                              )}
                            </div>
                            <div className="shop-meta__section">
                              <span className="shop-meta__title">Table totals</span>
                              <span>Chips: {formatNumber(totalChips ?? 0)}</span>
                              <span>Straps: {formatNumber(totalStrapBets)}</span>
                            </div>
                          </div>
                        ) : null}
                        {isExpanded ? (
                          <div className="shop-window__details">
                            <div className="shop-window__section">
                              <h3>Rewards</h3>
                              {rewards.length > 0 ? (
                                <div className="shop-window__stack">
                                  {rewards.map(([strap, amount], rewardIndex) => (
                                    <div key={`${roll}-reward-${rewardIndex}`}>
                                      {formatRewardCompact(strap)} ·{" "}
                                      {formatNumber(amount)}
                                    </div>
                                  ))}
                                </div>
                              ) : (
                                <div className="shop-window__muted">
                                  None for this shop.
                                </div>
                              )}
                            </div>
                            <div className="shop-window__section">
                              <h3>Your bets</h3>
                              {accountBetDetails.length > 0 ? (
                                <div className="shop-window__stack">
                                  {accountBetDetails.map((detail, detailIndex) => (
                                    <div key={`account-bet-${roll}-${detailIndex}`}>
                                      {detail.kind === "chip"
                                        ? `Chip x${formatNumber(detail.amount)}`
                                        : `${formatRewardCompact(detail.strap)} x${formatNumber(
                                            detail.amount
                                          )}`}
                                      {typeof detail.betRollIndex === "number"
                                        ? ` @${detail.betRollIndex}`
                                        : ""}
                                    </div>
                                  ))}
                                </div>
                              ) : (
                                <div className="shop-window__muted">
                                  No bets yet.
                                </div>
                              )}
                            </div>
                            <div className="shop-window__section">
                              <h3>Modifiers</h3>
                              {modifier ? (
                                <div className="shop-window__stack">
                                  <div>
                                    {modifierEmojis[modifier] ?? ""} {modifier}
                                  </div>
                                </div>
                              ) : (
                                <div className="shop-window__muted">
                                  None active.
                                </div>
                              )}
                            </div>
                            <div className="shop-window__section shop-window__section--wide">
                              <h3>Table bets</h3>
                              <div className="shop-window__table-summary">
                                <span>
                                  Chips: {formatNumber(totalChips ?? 0)}
                                </span>
                                {tableStrapTotals.length > 0 ? (
                                  <div className="shop-window__table-straps">
                                    {tableStrapTotals.map(({ strap, amount }) => (
                                      <span key={`strap-summary-${strapKey(strap)}`}>
                                        {formatRewardCompact(strap)} ·{" "}
                                        {formatNumber(amount)}
                                      </span>
                                    ))}
                                  </div>
                                ) : (
                                  <span>Straps: {formatNumber(totalStrapBets)}</span>
                                )}
                              </div>
                              {tableBetsForRoll.length > 0 ? (
                                <div className="shop-window__table-scroll">
                                  <div className="shop-window__table">
                                    {tableBetsForRoll.map((entry, tableIndex) => (
                                      <div
                                        key={`${roll}-table-${tableIndex}`}
                                        className="shop-window__table-entry"
                                      >
                                        <div className="shop-window__address">
                                          address:{" "}
                                          {formatIdentity(entry.identity).toLowerCase()}
                                        </div>
                                        <div className="shop-window__stack">
                                          <div>
                                            Chip bets:{" "}
                                            {formatNumber(entry.chipTotal)}
                                          </div>
                                          {entry.straps.length > 0 ? (
                                            <div className="shop-window__stack">
                                              {entry.straps.map(
                                                ([strap, amount], strapIndex) => (
                                                  <div
                                                    key={`${roll}-table-${tableIndex}-strap-${strapIndex}`}
                                                  >
                                                    {formatRewardCompact(strap)} ·{" "}
                                                    {formatNumber(amount)}
                                                  </div>
                                                )
                                              )}
                                            </div>
                                          ) : (
                                            <div className="shop-window__muted">
                                              No strap bets.
                                            </div>
                                          )}
                                        </div>
                                      </div>
                                    ))}
                                  </div>
                                </div>
                              ) : (
                                <div className="shop-window__muted">
                                  No table bets yet.
                                </div>
                              )}
                            </div>
                          </div>
                        ) : null}
                      </div>
                      <button
                        type="button"
                        className="shop-door"
                        aria-label={`Place bet on ${rollLabels[roll]}`}
                        onClick={(event) => {
                          event.stopPropagation();
                          openBetModal(roll);
                        }}
                      >
                        <span className="shop-door__label">Bet</span>
                      </button>
                    </div>
                    {!isExpanded && rewards.length > 0 ? (
                      <div
                        className={`shop-dangling-stack${
                          rewards.length > 1 ? " shop-dangling-stack--raised" : ""
                        }`}
                      >
                        {rewards.map(([strap, amount], rewardIndex) => (
                          <div
                            key={`${roll}-dangling-${rewardIndex}`}
                            className="shop-dangling"
                            style={
                              {
                                "--dangling-index": rewardIndex,
                              } as React.CSSProperties
                            }
                          >
                            <span className="shop-dangling__emoji">
                              {formatRewardCompact(strap)}
                            </span>
                            <span className="shop-dangling__price">
                              {formatNumber(amount)}
                            </span>
                          </div>
                        ))}
                      </div>
                    ) : null}
                    <div className="shop-glow" />
                    {modifierStory ? (
                      <div
                        className={`modifier-aura modifier-aura--${modifierStory.theme}`}
                      />
                    ) : null}
                  </div>
                </div>
                <div className="modifier-stack">
                  {modifierStory && shouldShowModifierBanner ? (
                    <div
                      className={`modifier-banner modifier-banner--${modifierStory.theme}`}
                    >
                      <span className="modifier-banner__icon" aria-hidden="true">
                        {modifierStory.icon}
                      </span>
                      <span className="modifier-banner__text">
                        {modifierStory.applied}
                      </span>
                    </div>
                  ) : null}
                  {visibleEntries.map((entry, entryIndex) => {
                    const story = modifierStories[entry.modifier];
                    if (!story) {
                      return null;
                    }
                    if (!entry.triggered) {
                      const triggerNumber = rollNumbers[entry.trigger_roll] ?? "—";
                      return (
                        <button
                          key={`${roll}-${entry.modifier}-${entryIndex}`}
                          type="button"
                          className="modifier-action modifier-action--locked"
                          disabled
                        >
                          <span className="modifier-action__icon" aria-hidden="true">
                            🔒
                          </span>
                          <span className="modifier-action__text">
                            roll {triggerNumber} to unlock modifier
                          </span>
                        </button>
                      );
                    }
                    const entryKey = `${roll}-${entry.modifier}-${entryIndex}`;
                    const isPurchasing =
                      modifierPurchaseKey === entryKey &&
                      (modifierPurchaseStatus === "signing" ||
                        modifierPurchaseStatus === "pending");
                    const actionText =
                      modifierPurchaseKey === entryKey &&
                      modifierPurchaseStatus === "error"
                        ? modifierPurchaseError ?? "Purchase failed"
                        : modifierPurchaseKey === entryKey &&
                          modifierPurchaseStatus === "success"
                          ? "Purchased"
                          : `${story.cta} ${formatNumber(entry.price)}`;
                    return (
                      <button
                        key={entryKey}
                        type="button"
                        className={`modifier-action modifier-action--${story.theme}`}
                        disabled={isPurchasing}
                        onClick={() => handlePurchaseModifier(roll, entry, entryKey)}
                      >
                        <span className="modifier-action__icon" aria-hidden="true">
                          {story.icon}
                        </span>
                        <span className="modifier-action__text">
                          {actionText}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </section>
      </main>

      {activeRoll ? (
        <button
          type="button"
          className="shop-overlay"
          aria-label="Close shop"
          onClick={() => setActiveRoll(null)}
        />
      ) : null}

      {betTargetRoll ? (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Bet slip</div>
                <h2 className="modal__title">
                  Bet on {rollLabels[betTargetRoll]}
                </h2>
              </div>
              <button className="ghost-button" type="button" onClick={closeBetModal}>
                Close
              </button>
            </div>
            <div className="modal__body">
              <div className="modal-card modal-card--wide">
                {betStatus === "success" ? (
                  <div className="bet-form">
                    <div className="bet-field">
                      <span className="bet-label">Success</span>
                      <div className="bet-success">
                        You placed{" "}
                        {betKind === "chip"
                          ? `${betAmount} chip(s)`
                          : selectedBetStrap
                          ? `${formatRewardCompact(selectedBetStrap.strap)} x${betAmount}`
                          : `strap x${betAmount}`}{" "}
                        on {rollLabels[betTargetRoll]}.
                      </div>
                      {betTxId ? (
                        <div className="bet-muted">
                          Tx: {betTxId.slice(0, 8)}…
                        </div>
                      ) : null}
                    </div>
                    <div className="bet-actions">
                      <button
                        className="primary-button"
                        type="button"
                        onClick={closeBetModal}
                      >
                        Done
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="bet-form">
                    <div className="bet-field">
                      <span className="bet-label">Bet type</span>
                      <div className="bet-toggle">
                        <button
                          type="button"
                          className={`bet-toggle__button${
                            betKind === "chip" ? " bet-toggle__button--active" : ""
                          }`}
                          onClick={() => setBetKind("chip")}
                        >
                          Chip
                        </button>
                        <button
                          type="button"
                          className={`bet-toggle__button${
                            betKind === "strap" ? " bet-toggle__button--active" : ""
                          }`}
                          onClick={() => setBetKind("strap")}
                        >
                          Strap
                        </button>
                      </div>
                    </div>
                    {betKind === "strap" ? (
                      <div className="bet-field">
                        <span className="bet-label">Select strap</span>
                        {ownedStraps.length > 0 ? (
                          <>
                            <button
                              type="button"
                              className="bet-variant-button"
                              onClick={() => setIsStrapKindPickerOpen(true)}
                              disabled={closetGroups.length === 0}
                            >
                              {selectedBetGroup
                                ? `${selectedBetGroup.emoji} ${selectedBetGroup.kind}`
                                : "Choose type"}
                            </button>
                            <button
                              type="button"
                              className="bet-variant-button"
                              onClick={() => setIsStrapPickerOpen(true)}
                              disabled={!selectedBetGroup}
                            >
                              {selectedBetStrap
                                ? `${formatRewardCompact(
                                    selectedBetStrap.strap
                                  )} · ${formatQuantity(selectedBetStrap.amount)}`
                                : "Choose variant"}
                            </button>
                          </>
                        ) : (
                          <div className="bet-muted">No straps owned.</div>
                        )}
                      </div>
                    ) : null}
                    <div className="bet-field">
                      <span className="bet-label">Amount</span>
                      <input
                        className="bet-input"
                        type="number"
                        min="1"
                        step="1"
                        inputMode="numeric"
                        value={betAmount}
                        onChange={(event) => setBetAmount(event.target.value)}
                      />
                      <span className="bet-help">
                        {betKind === "chip"
                          ? `Using ${chipAssetTicker}`
                          : "Strap quantity"}
                      </span>
                    </div>
                    <div className="bet-actions">
                      <div className="bet-status">
                        {betStatusMessage ? betStatusMessage : "Ready to bet."}
                        {betTxId ? ` (${betTxId.slice(0, 8)}…)` : ""}
                      </div>
                      <button
                        className="primary-button"
                        type="button"
                        onClick={handlePlaceBet}
                        disabled={
                          isBetBusy ||
                          (betKind === "strap" && ownedStraps.length === 0)
                        }
                      >
                        {isBetBusy ? "Submitting..." : "Place bet"}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      ) : null}

      {isStrapKindPickerOpen && betKind === "strap" ? (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal modal--tall">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Straps</div>
                <h2 className="modal__title">Choose type</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsStrapKindPickerOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body modal__body--scroll">
              {closetGroups.length > 0 ? (
                <div className="bet-kind-grid">
                  {closetGroups.map((group) => {
                    const isActive = group.kind === betStrapKind;
                    return (
                      <button
                        key={`bet-kind-${group.kind}`}
                        type="button"
                        className={`bet-kind${isActive ? " bet-kind--active" : ""}`}
                        onClick={() => {
                          setBetStrapKind(group.kind);
                          setIsStrapKindPickerOpen(false);
                        }}
                      >
                        <div className="bet-kind__icon" aria-hidden="true">
                          {group.emoji}
                        </div>
                        <div className="bet-kind__label">{group.kind}</div>
                        <div className="bet-kind__meta">
                          {group.entries.length} variants
                        </div>
                      </button>
                    );
                  })}
                </div>
              ) : (
                <div className="modal-muted">No straps available.</div>
              )}
            </div>
          </div>
        </div>
      ) : null}

      {isStrapPickerOpen && betKind === "strap" ? (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal modal--tall">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Straps</div>
                <h2 className="modal__title">Choose variant</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsStrapPickerOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body modal__body--scroll">
              {selectedBetGroup ? (
                <div className="closet-kind">
                  <div className="closet-kind__badge">
                    <div className="closet-kind__icon" aria-hidden="true">
                      {selectedBetGroup.emoji}
                    </div>
                    <div>{selectedBetGroup.kind}</div>
                  </div>
                  <div className="closet-kind__items">
                    {selectedBetGroup.entries.map((entry) => {
                      const isActive = entry.assetId === betStrapAssetId;
                      return (
                        <button
                          key={`bet-variant-${entry.assetId}`}
                          type="button"
                          className={`bet-variant${
                            isActive ? " bet-variant--active" : ""
                          }`}
                          onClick={() => {
                            setBetStrapAssetId(entry.assetId);
                            setIsStrapPickerOpen(false);
                          }}
                        >
                          <div className="bet-variant__title">
                            {formatRewardCompact(entry.strap)}
                          </div>
                          <div className="bet-variant__meta">
                            L{entry.strap.level} · {formatQuantity(entry.amount)}
                          </div>
                          <div className="bet-variant__asset">
                            Asset: {formatAssetId(entry.assetId)}
                          </div>
                        </button>
                      );
                    })}
                  </div>
                </div>
              ) : (
                <div className="modal-muted">No straps available.</div>
              )}
            </div>
          </div>
        </div>
      ) : null}

      {isClosetOpen && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal modal--tall">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Wardrobe</div>
                <h2 className="modal__title">Closet</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsClosetOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body modal__body--scroll">
              {ownedStraps.length > 0 ? (
                <div className="closet-list">
                  {closetGroups.map((group) => (
                    <div key={`closet-kind-${group.kind}`} className="closet-kind">
                      <div className="closet-kind__badge">
                        <div className="closet-kind__icon" aria-hidden="true">
                          {group.emoji}
                        </div>
                      </div>
                      <div className="closet-kind__items">
                        {group.entries.map((entry) => (
                          <div
                            key={`closet-${entry.assetId}`}
                            className="closet-variant"
                          >
                            <div className="closet-variant__title">
                              {modifierEmojis[entry.strap.modifier] ?? ""}
                              L{entry.strap.level} · {formatQuantity(entry.amount)}
                            </div>
                            <div className="closet-variant__asset">
                              Asset: {formatAssetId(entry.assetId)}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="modal-muted">No straps yet.</div>
              )}
            </div>
          </div>
        </div>
      )}

      <footer className="street-footer">
        <div className="footer-actions">
          <div className="chips-banner">
            {formatChipUnits(displayChipBalance)} CHIPS
          </div>
          <button
            className="ghost-button"
            type="button"
            onClick={() => setIsClosetOpen(true)}
          >
            STRAPS CLOSET
          </button>
          <button
            className="ghost-button"
            type="button"
            onClick={() => setIsGamesOpen(true)}
          >
            Previous games
          </button>
          <button
            className="ghost-button"
            type="button"
            onClick={() => setIsInfoOpen(true)}
          >
            Game info
          </button>
        </div>
        <div className="last-updated">
          {lastUpdated ? `Updated ${lastUpdated}` : "Not updated yet"}
          {error ? ` · ⚠️ ${error}` : ""}
        </div>
      </footer>

      {isGamesOpen && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Archive</div>
                <h2 className="modal__title">Previous games</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsGamesOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body modal__body--scroll">
              <div className="modal-card modal-card--wide">
                <h3>Unclaimed history</h3>
                {gameHistory.length > 0 ? (
                  <div className="history-list">
                    {gameHistory.map((entry) => {
                      const accountBets = entry.account?.per_roll_bets ?? [];
                      const accountBetsSummary = summarizeBetsByKind(
                        accountBets.flatMap((bets) => bets.bets)
                      );
                      const accountBetDetails = accountBets.flatMap((rollEntry) =>
                        rollEntry.bets
                          .map((bet) => {
                            const parsed = parseAccountBetPlacement(bet);
                            if (!parsed) {
                              return null;
                            }
                            return {
                              roll: rollEntry.roll,
                              kind: parsed.kind,
                              amount: parsed.amount,
                              strap: parsed.kind === "strap" ? parsed.strap : undefined,
                              betRollIndex: parsed.betRollIndex,
                            };
                          })
                          .filter(
                            (
                              bet
                            ): bet is {
                              roll: Roll;
                              kind: "chip" | "strap";
                              amount: number;
                              strap: Strap | undefined;
                              betRollIndex: number | undefined;
                            } => bet !== null
                          )
                      );
                      const hasClaimable = hasClaimableBets(entry.rolls, accountBets);
                      const claimedLabel = entry.claimed
                        ? "Claimed"
                        : hasClaimable
                        ? "Unclaimed"
                        : "Nothing to claim";
                      const canClaim = !entry.claimed && hasClaimable;
                      const isClaimingGame = claimGameId === entry.gameId;
                      const claimButtonLabel = isClaimingGame
                        ? claimStatus === "signing"
                          ? "Signing..."
                          : claimStatus === "pending"
                          ? "Claiming..."
                          : claimStatus === "error"
                          ? "Claim failed"
                          : claimStatus === "success"
                          ? "Claimed"
                          : "Claim"
                        : "Claim";
                      const claimDisabled =
                        !canClaim || (isClaimBusy && !isClaimingGame);
                      const handleClaimClick = () => {
                        const eligibleModifiers = canClaim
                          ? eligibleClaimModifiers(entry)
                          : [];
                        if (eligibleModifiers.length > 0) {
                          setClaimModifierEntry(entry);
                          setClaimModifierOptions(eligibleModifiers);
                          setClaimModifierSelection(
                            eligibleModifiers.map(
                              (modifier) =>
                                `${modifier.modifierRoll}:${modifier.modifier}:${modifier.rollIndex}`
                            )
                          );
                          return;
                        }
                        handleClaimRewards(entry, []);
                      };
                      return (
                        <div
                          key={`history-${entry.gameId}`}
                          className="history-item"
                        >
                          <div className="history-item__header">
                            <div>
                              <div className="history-item__title">
                                Game {entry.gameId}
                              </div>
                              <div className="history-item__status">
                                {claimedLabel}
                              </div>
                            </div>
                            <button
                              className="primary-button"
                              type="button"
                              disabled={claimDisabled}
                              onClick={handleClaimClick}
                              title={
                                isClaimingGame && claimStatus === "error"
                                  ? claimError ?? undefined
                                  : undefined
                              }
                            >
                              {claimButtonLabel}
                            </button>
                          </div>
                          <div className="history-item__detail">
                            <span>Rolls:</span>{" "}
                            {entry.rolls.length > 0
                              ? entry.rolls
                                  .map((roll, index) => {
                                    const activeModifiers = entry.modifiers
                                      .filter(
                                        (modifier) =>
                                          modifier.modifierRoll === roll &&
                                          modifier.rollIndex <= index
                                      )
                                      .map(
                                        (modifier) =>
                                          modifierEmojis[modifier.modifier] ?? ""
                                      )
                                      .filter(Boolean)
                                      .join("");
                                    return `${rollLabels[roll]}${
                                      activeModifiers ? ` ${activeModifiers}` : ""
                                    }`;
                                  })
                                  .join(", ")
                              : "—"}
                          </div>
                          <div className="history-item__detail">
                            <span>Rewards:</span>{" "}
                            {entry.strapRewards.length > 0
                              ? entry.strapRewards
                                  .map(
                                    ([roll, strap, amount]) =>
                                      `${rollLabels[roll]} ${formatRewardCompact(
                                        strap
                                      )}/${formatNumber(amount)}`
                                  )
                                  .join(", ")
                              : "—"}
                          </div>
                          <div className="history-item__detail">
                            <span>Your bets:</span>{" "}
                            {accountBetsSummary.chipTotal > 0 ||
                            accountBetsSummary.straps.length > 0
                              ? [
                                  accountBetsSummary.chipTotal > 0
                                    ? `${formatNumber(
                                        accountBetsSummary.chipTotal
                                      )} chips`
                                    : null,
                                  ...accountBetsSummary.straps.map(
                                    ([strap, amount]) =>
                                      `${formatRewardCompact(strap)} x${formatNumber(
                                        amount
                                      )}`
                                  ),
                                ]
                                  .filter(Boolean)
                                  .join(", ")
                              : "—"}
                          </div>
                          {accountBetDetails.length > 0 ? (
                            <div className="history-item__detail history-item__detail--tight">
                              <span>Bets detail:</span>
                              <div className="history-bets">
                                {accountBetDetails.map((bet, index) => {
                                  const betIndexLabel =
                                    typeof bet.betRollIndex === "number"
                                      ? ` @${bet.betRollIndex}`
                                      : "";
                                  const betLabel =
                                    bet.kind === "chip"
                                      ? `${formatNumber(bet.amount)} chips`
                                      : bet.strap
                                      ? `${formatRewardCompact(
                                          bet.strap
                                        )} x${formatNumber(bet.amount)}`
                                      : `strap x${formatNumber(bet.amount)}`;
                                  return (
                                    <div
                                      key={`history-bet-${entry.gameId}-${index}`}
                                      className="history-bet-line"
                                    >
                                      {rollLabels[bet.roll]}: {betLabel}
                                      {betIndexLabel}
                                    </div>
                                  );
                                })}
                              </div>
                            </div>
                          ) : null}
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <div className="modal-muted">No previous games yet.</div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {isInfoOpen && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Game</div>
                <h2 className="modal__title">Game details</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsInfoOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body">
              <div className="modal-card">
                <h3>Status</h3>
                <div className="modal-stack">
                  <div>Game: {snapshot ? snapshot.game_id : "—"}</div>
                  <div>Pot: {snapshot ? formatNumber(snapshot.pot_size) : "—"}</div>
                  <div>Owed: {snapshot ? formatNumber(snapshot.chips_owed) : "—"}</div>
                  <div>
                    Chip bets:{" "}
                    {snapshot ? formatNumber(snapshot.total_chip_bets) : "—"}
                  </div>
                </div>
              </div>
              <div className="modal-card">
                <h3>Assets</h3>
                <div className="modal-stack">
                  <div>
                    Base: {formatAssetLabel(baseAssetId, baseAssetTicker)}
                  </div>
                  <div>
                    Chips: {formatAssetLabel(chipAssetId, chipAssetTicker)}
                  </div>
                </div>
              </div>
              <div className="modal-card">
                <h3>Chain</h3>
                <div className="modal-stack">
                  <div>
                    Block:{" "}
                    {snapshot ? formatNumber(snapshot.current_block_height) : "—"}
                  </div>
                  <div>
                    Next roll:{" "}
                    {snapshot ? formatNumber(snapshot.next_roll_height) : "—"}
                  </div>
                  <div>
                    Roll freq:{" "}
                    {snapshot ? formatNumber(snapshot.roll_frequency) : "—"}
                  </div>
                </div>
              </div>
              <div className="modal-card">
                <h3>Shop</h3>
                {shopEntries.length > 0 ? (
                  <div className="modal-stack">
                    {shopEntries.slice(0, 6).map((entry, index) => (
                      <div key={`shop-modal-${index}`}>
                        {entry.modifier_roll} · {entry.modifier} ·{" "}
                        {formatNumber(entry.price)}
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="modal-muted">No shop entries yet.</div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

      {claimModifierEntry && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Claim</div>
                <h2 className="modal__title">
                  Apply modifiers for game {claimModifierEntry.gameId}?
                </h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={closeClaimModifier}
              >
                Close
              </button>
            </div>
            <div className="modal__body">
              <div className="modal-card modal-card--wide">
                <div className="modal-stack">
                  {claimModifierOptions.map((modifier, index) => {
                    const key = `${modifier.modifierRoll}:${modifier.modifier}:${modifier.rollIndex}`;
                    const isSelected = claimModifierSelection.includes(key);
                    return (
                      <label
                        key={`claim-mod-${key}-${index}`}
                        className="claim-modifier"
                      >
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={(event) => {
                            const checked = event.target.checked;
                            setClaimModifierSelection((prev) =>
                              checked
                                ? [...prev, key]
                                : prev.filter((value) => value !== key)
                            );
                          }}
                        />
                        <span className="claim-modifier__box" aria-hidden="true" />
                        <span className="claim-modifier__label">
                          {modifierEmojis[modifier.modifier] ?? ""}{" "}
                          {modifier.modifier} · {rollLabels[modifier.modifierRoll]} @
                          {modifier.rollIndex}
                        </span>
                      </label>
                    );
                  })}
                </div>
                <div className="modal-actions">
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={closeClaimModifier}
                  >
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    type="button"
                    onClick={() => {
                      const enabledModifiers = claimModifierOptions
                        .filter((modifier) =>
                          claimModifierSelection.includes(
                            `${modifier.modifierRoll}:${modifier.modifier}:${modifier.rollIndex}`
                          )
                        )
                        .map((modifier) => [
                          modifier.modifierRoll,
                          modifier.modifier,
                        ]) as Array<[Roll, string]>;
                      closeClaimModifier();
                      handleClaimRewards(claimModifierEntry, enabledModifiers);
                    }}
                  >
                    Claim rewards
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {claimResult && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal modal--claim-result">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Claim</div>
                <h2 className="modal__title">
                  Claimed game {claimResult.gameId}
                </h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={closeClaimResult}
              >
                Close
              </button>
            </div>
            <div className="modal__body">
              <div className="modal-card modal-card--wide">
                <div className="modal-stack">
                  <div className="modal-row">
                    <span>Chips won</span>
                    <span>
                      {claimResult.chipWon === null
                        ? "Awaiting indexer"
                        : formatNumber(claimResult.chipWon)}
                    </span>
                  </div>
                  <div className="modal-row">
                    <span>Bet total</span>
                    <span>{formatNumber(claimResult.chipBet)}</span>
                  </div>
                  <div className="modal-row">
                    <span>Net</span>
                    <span>
                      {claimResult.chipWon === null || claimResult.netChip === null
                        ? "—"
                        : `${claimResult.netChip >= 0 ? "+" : "-"}${formatNumber(
                            Math.abs(claimResult.netChip)
                          )}`}
                    </span>
                  </div>
                  <div className="modal-row">
                    <span>Your bets</span>
                    <span>
                      {claimResult.betDetails.length > 0
                        ? "See list"
                        : "No bets recorded."}
                    </span>
                  </div>
                  {claimResult.betDetails.length > 0 ? (
                    <div className="modal-bets">
                      {claimResult.betDetails.map((bet, index) => {
                        const rollLabel = rollLabels[bet.roll];
                        const betIndexLabel =
                          typeof bet.betRollIndex === "number"
                            ? ` @${bet.betRollIndex}`
                            : "";
                        const betLabel =
                          bet.kind === "chip"
                            ? `${formatNumber(bet.amount)} chips`
                            : bet.strap
                            ? `${formatRewardCompact(bet.strap)} x${formatNumber(
                                bet.amount
                              )}`
                            : `strap x${formatNumber(bet.amount)}`;
                        return (
                          <div
                            key={`claim-bet-${claimResult.gameId}-${index}`}
                            className="modal-bet-line"
                          >
                            {rollLabel}: {betLabel}
                            {betIndexLabel}
                          </div>
                        );
                      })}
                    </div>
                  ) : null}
                  <div className="modal-row">
                    <span>Straps</span>
                    <span>
                      {claimResult.straps.length > 0
                        ? claimResult.straps
                            .map(
                              ([strap, amount]) =>
                                `${formatRewardCompact(strap)} x${formatNumber(amount)}`
                            )
                            .join(", ")
                        : "No strap rewards yet."}
                    </span>
                  </div>
                  {claimResult.txId ? (
                    <div className="modal-muted">
                      Tx: {claimResult.txId.slice(0, 8)}…
                    </div>
                  ) : null}
                  {claimResult.chipWon === null ? (
                    <div className="modal-muted">
                      Rewards may take a moment to index.
                    </div>
                  ) : null}
                </div>
                <div className="modal-actions">
                  <button
                    className="primary-button"
                    type="button"
                    onClick={closeClaimResult}
                  >
                    Done
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {isDiceHistoryOpen && (
        <div className="modal-overlay" role="dialog" aria-modal="true">
          <div className="modal">
            <div className="modal__header">
              <div>
                <div className="modal__eyebrow">Dice</div>
                <h2 className="modal__title">Roll history</h2>
              </div>
              <button
                className="ghost-button"
                type="button"
                onClick={() => setIsDiceHistoryOpen(false)}
              >
                Close
              </button>
            </div>
            <div className="modal__body">
              <div className="modal-card">
                {(snapshot?.rolls ?? []).length > 0 ? (
                  <div className="roll-history-grid">
                    {snapshot?.rolls
                      .slice()
                      .reverse()
                      .map((roll, index) => (
                        <div
                          key={`roll-history-${roll}-${index}`}
                          className="roll-history-item"
                        >
                          <span className="roll-history-index">
                            #{snapshot?.rolls.length - index}
                          </span>
                          <span>
                            {rollNumbers[roll]} · {rollLabels[roll]}
                          </span>
                        </div>
                      ))}
                  </div>
                ) : (
                  <div className="modal-muted">No rolls yet.</div>
                )}
              </div>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}
