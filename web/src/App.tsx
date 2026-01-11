import { useEffect, useMemo, useState } from "react";

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

const modifierBorderColors: Record<string, string> = {
  Nothing: "#6c757d",
  Burnt: "#dc3545",
  Lucky: "#28a745",
  Holy: "#ffc107",
  Holey: "#6c757d",
  Scotch: "#8b572a",
  Soaked: "#007bff",
  Moldy: "#6f42c1",
  Starched: "#dee2e6",
  Evil: "#9c27b0",
  Groovy: "#ff5722",
  Delicate: "#ffb6c1",
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
  bets: [unknown, number, number][];
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
          bets: Array.isArray(bets) ? (bets as [unknown, number, number][]) : [],
        };
      }

      if (entry && typeof entry === "object") {
        const obj = entry as Record<string, unknown>;
        if ("roll" in obj && "bets" in obj) {
          return {
            roll: obj.roll as Roll,
            bets: Array.isArray(obj.bets)
              ? (obj.bets as [unknown, number, number][])
              : [],
          };
        }
      }

      return null;
    })
    .filter((entry): entry is NormalizedRollBets => entry !== null);
}

function sumBetAmounts(bets: unknown[]): number {
  return bets.reduce<number>((sum, bet) => {
    if (Array.isArray(bet)) {
      const numeric = bet.find((value) => typeof value === "number");
      return typeof numeric === "number" ? sum + numeric : sum;
    }
    if (bet && typeof bet === "object") {
      const record = bet as Record<string, unknown>;
      const amount = record.amount;
      return typeof amount === "number" ? sum + amount : sum;
    }
    return sum;
  }, 0);
}

export default function App() {
  const baseUrl = useMemo(
    () => normalizeBaseUrl(import.meta.env.VITE_INDEXER_URL as string | undefined),
    []
  );
  const [status, setStatus] = useState<FetchStatus>("idle");
  const [error, setError] = useState<string | null>(null);
  const [data, setData] = useState<SnapshotResponse | null>(null);
  const [lastUpdated, setLastUpdated] = useState<string | null>(null);

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

  const formatRewardCompact = (strap: Strap) => {
    const modifierEmoji = modifierEmojis[strap.modifier] ?? "";
    const strapEmoji = strapEmojis[strap.kind] ?? "🎽";
    const level = strap.level;
    return `${modifierEmoji}${strapEmoji}${level}`;
  };

  const formatNumber = (value: number | null | undefined) =>
    value === null || value === undefined ? "—" : value.toLocaleString();

  return (
    <div className="app">
      <section className="panel panel--tight panel--top">
        <div className="panel__header">
          <div className="app__title-row">
            <div className="app__title">Strapped</div>
            <div className="game-pill">
              Game: {snapshot ? snapshot.game_id : "—"}
            </div>
          </div>
          <div className="app__status">
            <span className={`pill pill--${status}`}>{status}</span>
            <span className="app__updated">
              {lastUpdated ? `Last update ${lastUpdated}` : "Not updated yet"}
            </span>
            {error && (
              <span className="error">
                <span>⚠️</span>
                <span>{error}</span>
              </span>
            )}
          </div>
        </div>
        <div className="panel__body">
          <div className="status-line">
            <div className="stat-item">
              Pot: {snapshot ? formatNumber(snapshot.pot_size) : "—"}
            </div>
            <div className="stat-item">
              Owed: {snapshot ? formatNumber(snapshot.chips_owed) : "—"}
            </div>
            <div className="stat-item">
              Chip Bets:{" "}
              {snapshot ? formatNumber(snapshot.total_chip_bets) : "—"}
            </div>
          </div>
          <div className="status-line">
            <div className="stat-item">
              Block:{" "}
              {snapshot ? formatNumber(snapshot.current_block_height) : "—"}
            </div>
            <div className="stat-item">
              Next Roll:{" "}
              {snapshot ? formatNumber(snapshot.next_roll_height) : "—"}
            </div>
            <div className="stat-item">
              Roll Freq:{" "}
              {snapshot ? formatNumber(snapshot.roll_frequency) : "—"}
            </div>
          </div>
          <div className="status-line">
            <div className="stat-item stat-item--label">Roll History:</div>
            <div className="roll-history roll-history--inline">
              {snapshot && snapshot.rolls.length > 0
                ? snapshot.rolls.map((roll, index) => (
                    <span key={`${roll}-${index}`} className="roll-pill">
                      {roll}
                    </span>
                  ))
                : "None"}
            </div>
          </div>
        </div>
      </section>

      <section className="panel panel--tight">
        <h2 className="panel__title">Wallet</h2>
        <div className="panel__body">
          <div className="stat-line">
            <div className="stat-item">Balance: —</div>
            <div className="stat-item">Chips: —</div>
            <div className="stat-item">Straps: —</div>
          </div>
        </div>
      </section>

      <section className="roll-grid">
        {rollOrder.map((roll, index) => {
          const rollBets = snapshot?.specific_bets?.[index];
          const totalChips = rollBets ? rollBets[0] : null;
          const strapBets = rollBets ? rollBets[1] : [];
          const rewards = rewardsByRoll.get(roll) ?? [];
          const modifier = snapshot?.modifiers_active?.[index] ?? null;
          const modifierEmoji = modifier ? modifierEmojis[modifier] ?? "" : "";
          const borderColor = modifier
            ? modifierBorderColors[modifier] ?? modifierBorderColors.Nothing
            : modifierBorderColors.Nothing;

          return (
            <div
              key={roll}
              className="roll-card"
              style={{ borderColor }}
            >
              <div className="roll-card__title">
                {rollLabels[roll]}
                {modifierEmoji ? ` ${modifierEmoji}` : ""}
              </div>
              <div className="roll-card__section">
                <div className="roll-card__label">Rewards</div>
                {rewards.length > 0 ? (
                  rewards.map(([strap, amount], rewardIndex) => (
                    <div key={`${roll}-reward-${rewardIndex}`}>
                    {formatRewardCompact(strap)} {formatNumber(amount)}
                  </div>
                ))
              ) : (
                <div>None</div>
              )}
            </div>
            <div className="roll-card__section">
              <div className="roll-card__label">You</div>
              <div>—</div>
            </div>
              <div className="roll-card__section">
                <div className="roll-card__label">Table</div>
                <div>{totalChips !== null ? formatNumber(totalChips) : "—"}</div>
                {strapBets.length > 0 && (
                  <div className="roll-card__stack">
                    {strapBets.map(([strap, amount], strapIndex) => (
                      <div key={`${roll}-strap-${strapIndex}`}>
                        {formatRewardCompact(strap)} {formatNumber(amount)}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </section>

      <section className="grid grid--bottom">
        <div className="panel panel--scroll">
          <h2 className="panel__title">Shop</h2>
          <div className="panel__body">
            {shopEntries.length > 0 ? (
              <div className="list">
                {shopEntries.map((entry, index) => {
                  const modifierEmoji = modifierEmojis[entry.modifier] ?? "";
                  const modifierLabel = modifierEmoji
                    ? `${modifierEmoji} `
                    : "";
                  let text = "";
                  if (entry.purchased) {
                    text = `${entry.modifier_roll} ${modifierLabel}- purchased (${formatNumber(
                      entry.price
                    )} chips)`;
                  } else if (entry.triggered) {
                    text = `${entry.modifier_roll} ${modifierLabel}- ${formatNumber(
                      entry.price
                    )} chips`;
                  } else {
                    text = `${entry.modifier_roll} ${modifierLabel}(Unlock by rolling ${entry.trigger_roll}) - ${formatNumber(
                      entry.price
                    )} chips`;
                  }

                  return (
                    <div key={`shop-${index}`} className="list__row">
                      <div>{text}</div>
                    </div>
                  );
                })}
              </div>
            ) : (
              <div>None</div>
            )}
          </div>
        </div>
        <div className="panel panel--scroll">
          <h2 className="panel__title">Table Bets</h2>
          <div className="panel__body">
            {snapshot && snapshot.table_bets.length > 0 ? (
              <div className="list">
                {snapshot.table_bets.map((table, index) => {
                  const perRoll = normalizePerRollBets(table.per_roll_bets);
                  const rollLines = perRoll
                    .map(({ roll, bets }) => {
                      if (!Array.isArray(bets) || bets.length === 0) {
                        return null;
                      }
                      const total = sumBetAmounts(bets);
                      if (total === 0) {
                        return `${roll}: 0 chips`;
                      }
                      return `${roll}: ${formatNumber(total)} chips`;
                    })
                    .filter((line): line is string => line !== null);

                  return (
                    <div key={`table-${index}`} className="list__row">
                      <div className="list__headline">
                        Address: {formatIdentity(table.identity)}
                      </div>
                      {rollLines.length > 0 ? (
                        <div className="list__stack">
                          {rollLines.map((line) => (
                            <div key={line}>{line}</div>
                          ))}
                        </div>
                      ) : (
                        <div className="list__subtle">No bets</div>
                      )}
                    </div>
                  );
                })}
              </div>
            ) : (
              <div>None</div>
            )}
          </div>
        </div>
      </section>


      <section className="panel panel--help panel--tight">
        <h2 className="panel__title">Help</h2>
        <div className="panel__body">
          <div className="help-line">
            ⇧/⇩ select · b chip bet · t strap bet · m purchase · r roll · c
            claim · q quit
          </div>
        </div>
      </section>
    </div>
  );
}
