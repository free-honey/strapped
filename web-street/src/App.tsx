import { CSSProperties, useEffect, useMemo, useState } from "react";
import type { WalletUnlocked } from "fuels";
import {
  connectFuelWallet,
  createStrappedContract,
} from "./fuel/client";
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

const modifierEmojis: Record<string, string> = {
  Nothing: "",
  Burnt: "🧯",
  Lucky: "🍀",
  Holy: "👼",
  Holey: "🫥",
  Scotch: "🏴",
  Soaked: "🌊",
  Moldy: "🍄",
  Starched: "🏳️",
  Evil: "😈",
  Groovy: "✌️",
  Delicate: "❤️",
};

type ModifierStory = {
  cta: string;
  applied: string;
  icon: string;
  theme: string;
};

const modifierStories: Record<string, ModifierStory> = {
  Burnt: { cta: "commit arson", applied: "ablaze", icon: "🔥", theme: "burnt" },
  Lucky: { cta: "bury gold", applied: "charmed", icon: "🍀", theme: "lucky" },
  Holy: { cta: "bless shop", applied: "blessed", icon: "✝", theme: "holy" },
  Holey: { cta: "release moths", applied: "holey", icon: "🕳️", theme: "holey" },
  Scotch: { cta: "play bagpipes", applied: "oaked", icon: "🥃", theme: "scotch" },
  Soaked: { cta: "flood shop", applied: "soaked", icon: "💧", theme: "soaked" },
  Moldy: {
    cta: "leave clothes in washer",
    applied: "moldy",
    icon: "🍄",
    theme: "moldy",
  },
  Starched: { cta: "dump flour", applied: "starched", icon: "✨", theme: "starched" },
  Evil: { cta: "curse shop", applied: "cursed", icon: "😈", theme: "evil" },
  Groovy: { cta: "dump paint", applied: "groovy", icon: "🪩", theme: "groovy" },
  Delicate: {
    cta: "handle with care",
    applied: "delicate",
    icon: "🕊️",
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
  const [walletStatus, setWalletStatus] = useState<
    "idle" | "connecting" | "connected" | "error"
  >("idle");
  const [walletError, setWalletError] = useState<string | null>(null);
  const [walletAddress, setWalletAddress] = useState<string | null>(null);
  const [wallet, setWallet] = useState<WalletUnlocked | null>(null);
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

  const openExpandedShop = (
    roll: Roll,
    event: React.MouseEvent<HTMLButtonElement>
  ) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const targetWidth = Math.min(900, window.innerWidth * 0.92);
    const targetHeight = Math.min(560, window.innerHeight * 0.8);
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
  const isAnyModalOpen = Boolean(
    activeRoll || isGamesOpen || isInfoOpen || isDiceHistoryOpen
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
      setActiveRoll(null);
      setIsGamesOpen(false);
      setIsInfoOpen(false);
      setIsDiceHistoryOpen(false);
    };

    window.addEventListener("keydown", handleKeyDown);

    return () => {
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [isAnyModalOpen]);

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

  const snapshot = data?.snapshot ?? null;
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
    setWalletStatus("connecting");
    setWalletError(null);

    try {
      const connected = await connectFuelWallet(networkKey);
      setWallet(connected.wallet);
      setWalletAddress(connected.address);
      setWalletStatus("connected");
      return connected.wallet;
    } catch (err) {
      const message = err instanceof Error ? err.message : "Wallet error";
      setWalletStatus("error");
      setWalletError(message);
      return null;
    }
  };

  const handleRoll = async () => {
    setRollError(null);
    setRollTxId(null);

    let activeWallet = wallet;
    if (!activeWallet) {
      activeWallet = await connectWallet();
    }

    if (!activeWallet) {
      setRollStatus("error");
      return;
    }

    setRollStatus("signing");

    try {
      const contract = createStrappedContract(activeWallet, networkKey);
      const response = await contract.functions.roll_dice().call();
      setRollTxId(response.transactionId);
      setRollStatus("pending");
      await response.waitForResult();
      setRollStatus("success");
    } catch (err) {
      const message = err instanceof Error ? err.message : "Roll failed";
      setRollError(message);
      setRollStatus("error");
    }
  };

  const isRolling = rollStatus === "signing" || rollStatus === "pending";
  const rollButtonLabel = (() => {
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
          <button
            className="ghost-button"
            type="button"
            onClick={() => setIsInfoOpen(true)}
          >
            Game info
          </button>
        </div>
      </header>

      <main className="street-stage">
        <div className="sky-glow" />
        <div className="street-sun" />
        <div className="cloud cloud--one" />
        <div className="cloud cloud--two" />
        <div className="street-ground" />

        <button
          type="button"
          className="dice-strip"
          aria-label="Recent dice rolls"
          onClick={() => setIsDiceHistoryOpen(true)}
          disabled={diceRolls.length === 0}
        >
          {diceRolls.length > 0 ? (
            diceRolls.map((roll, index) => (
              <div
                key={`${roll}-${index}`}
                className="dice-card"
                style={
                  { "--depth": index, "--offset": index * 42 } as CSSProperties
                }
              >
                <div className="dice-face">{rollNumbers[roll]}</div>
                <div className="dice-label">{rollLabels[roll]}</div>
              </div>
            ))
          ) : (
            <div className="dice-placeholder">Waiting on rolls...</div>
          )}
        </button>

        <div className="roll-panel">
          <div className="roll-panel__row">
            <label className="roll-panel__label" htmlFor="network-select">
              Network
            </label>
            <select
              id="network-select"
              className="roll-panel__select"
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
          </div>
          <div className="roll-panel__row roll-panel__row--actions">
            <button
              className="ghost-button"
              type="button"
              onClick={connectWallet}
              disabled={walletStatus === "connecting"}
            >
              {walletStatus === "connecting" ? "Connecting..." : "Connect"}
            </button>
            <button
              className="primary-button roll-button"
              type="button"
              onClick={handleRoll}
              disabled={isRolling || walletStatus === "connecting"}
            >
              {rollButtonLabel}
            </button>
          </div>
          <div className="roll-panel__status">
            {walletError ? `Wallet: ${walletError}` : null}
            {!walletError && rollError ? `Roll: ${rollError}` : null}
            {!walletError && !rollError && rollTxId
              ? `Tx: ${rollTxId.slice(0, 10)}...${rollTxId.slice(-6)}`
              : null}
            {!walletError && !rollError && !rollTxId
              ? "Ready to roll on testnet."
              : null}
          </div>
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
            const visibleEntries = modifierEntries.filter(
              (entry) => !entry.purchased && entry.modifier !== modifier
            );
            const hasTableBets = Boolean(
              (totalChips ?? 0) > 0 || strapBets.length > 0
            );
            const totalStrapBets = strapBets.reduce(
              (sum, [, amount]) => sum + amount,
              0
            );
            const shopClassName = buildShopClasses({
              hasReward: rewards.length > 0,
              hasTableBets,
              modifier,
              index,
            });
            const isExpanded = activeRoll === roll;
            const danglingReward = rewards[0];
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

            return (
              <div key={roll} className="shop-cell">
                <div
                  className={`shop-frame${
                    isExpanded ? " shop-frame--expanded" : ""
                  }`}
                >
                  <button
                    type="button"
                    className={`${shopClassName}${isExpanded ? " shop-tile--expanded" : ""}${
                      isExpanded && isExpandedActive
                        ? " shop-tile--expanded-active"
                        : ""
                    }`}
                    onClick={(event) => openExpandedShop(roll, event)}
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
                          <span>Chips: {formatNumber(totalChips ?? 0)}</span>
                          <span>Straps: {formatNumber(totalStrapBets)}</span>
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
                            <div className="shop-window__muted">TBD</div>
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
                              <div className="shop-window__muted">None active.</div>
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
                                          Chip bets: {formatNumber(entry.chipTotal)}
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
                    <div className="shop-door" />
                  </div>
                  {!isExpanded && danglingReward ? (
                    <div className="shop-dangling">
                      <span className="shop-dangling__emoji">
                        {formatRewardCompact(danglingReward[0])}
                      </span>
                      <span className="shop-dangling__price">
                        {formatNumber(danglingReward[1])}
                      </span>
                    </div>
                  ) : null}
                  <div className="shop-glow" />
                  {modifierStory ? (
                    <div
                      className={`modifier-aura modifier-aura--${modifierStory.theme}`}
                    />
                  ) : null}
                  </button>
                </div>
                <div className="modifier-stack">
                  {modifierStory ? (
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
                    return (
                      <button
                        key={`${roll}-${entry.modifier}-${entryIndex}`}
                        type="button"
                        className={`modifier-action modifier-action--${story.theme}`}
                      >
                        <span className="modifier-action__icon" aria-hidden="true">
                          {story.icon}
                        </span>
                        <span className="modifier-action__text">
                          {story.cta} {formatNumber(entry.price)}
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

      <footer className="street-footer">
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
            <div className="modal__body">
              <div className="modal-card">
                <h3>Recent games</h3>
                <div className="modal-muted">Coming soon.</div>
              </div>
              <div className="modal-card">
                <h3>Claim rewards</h3>
                <div className="modal-muted">
                  Rewards will show here when available.
                </div>
                <button className="primary-button" type="button">
                  Claim all
                </button>
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
                <div className="modal-stack">
                  {(snapshot?.rolls ?? []).length > 0 ? (
                    snapshot?.rolls
                      .slice()
                      .reverse()
                      .map((roll, index) => (
                        <div key={`roll-history-${roll}-${index}`}>
                          {rollNumbers[roll]} · {rollLabels[roll]}
                        </div>
                      ))
                  ) : (
                    <div className="modal-muted">No rolls yet.</div>
                  )}
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

    </div>
  );
}
