import { CSSProperties, useEffect, useMemo, useState } from "react";

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

  const selectedRollIndex = activeRoll ? getRollIndex(activeRoll) : -1;
  const selectedRollBets =
    selectedRollIndex >= 0 ? snapshot?.specific_bets?.[selectedRollIndex] : null;
  const selectedTotalChips = selectedRollBets ? selectedRollBets[0] : null;
  const selectedStrapBets = selectedRollBets ? selectedRollBets[1] : [];
  const selectedRewards = activeRoll ? rewardsByRoll.get(activeRoll) ?? [] : [];
  const selectedModifier =
    selectedRollIndex >= 0 ? snapshot?.modifiers_active?.[selectedRollIndex] : null;
  const selectedModifierEmoji = selectedModifier
    ? modifierEmojis[selectedModifier] ?? ""
    : "";
  const selectedStrapTotal = selectedStrapBets.reduce(
    (sum, [, amount]) => sum + amount,
    0
  );
  const activeShopClass =
    selectedRollIndex >= 0 ? `shop-tile--${selectedRollIndex % 5}` : "";

  return (
    <div className="street-app">
      <header className="street-header">
        <h1 className="street-title">STRAPPED!</h1>
        <div className="street-meta">
          <span className={`status-chip status-chip--${status}`}>{status}</span>
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
            const danglingReward = rewards[0];

            return (
              <div key={roll} className="shop-cell">
                <button
                  type="button"
                  className={shopClassName}
                  onClick={() => setActiveRoll(roll)}
                >
                  <div className="shop-sign">
                    <span className="shop-sign__label">{rollLabels[roll]}</span>
                  </div>
                  <div className="shop-awning" />
                  <div className="shop-facade">
                    <div className="shop-window">
                      <div className="shop-meta">
                        <span>Chips: {formatNumber(totalChips ?? 0)}</span>
                        <span>Straps: {formatNumber(totalStrapBets)}</span>
                      </div>
                    </div>
                    <div className="shop-door" />
                  </div>
                  {danglingReward ? (
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

      {activeRoll && (
        <div className="modal-overlay modal-overlay--blur" role="dialog" aria-modal="true">
          <div className={`shop-focus ${activeShopClass}`}>
            <button
              className="ghost-button shop-focus__close"
              type="button"
              onClick={() => setActiveRoll(null)}
            >
              Close
            </button>
            <div className="shop-sign">
              <span className="shop-sign__label">{rollLabels[activeRoll]}</span>
            </div>
            <div className="shop-awning" />
            <div className="shop-focus__panel">
              <div className="shop-focus__window">
                <div className="shop-focus__section">
                  <h3>Totals</h3>
                  <div>Chips: {formatNumber(selectedTotalChips ?? 0)}</div>
                  <div>Straps: {formatNumber(selectedStrapTotal)}</div>
                </div>
                <div className="shop-focus__section">
                  <h3>Rewards</h3>
                  {selectedRewards.length > 0 ? (
                    <div className="shop-focus__stack">
                      {selectedRewards.map(([strap, amount], rewardIndex) => (
                        <div key={`${activeRoll}-reward-${rewardIndex}`}>
                          {formatRewardCompact(strap)} · {formatNumber(amount)}
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="shop-focus__muted">None for this shop.</div>
                  )}
                </div>
                <div className="shop-focus__section">
                  <h3>Table bets</h3>
                  {selectedStrapBets.length > 0 ? (
                    <div className="shop-focus__stack">
                      {selectedStrapBets.map(([strap, amount], strapIndex) => (
                        <div key={`${activeRoll}-strap-${strapIndex}`}>
                          {formatRewardCompact(strap)} · {formatNumber(amount)}
                        </div>
                      ))}
                    </div>
                  ) : (
                    <div className="shop-focus__muted">No strap bets.</div>
                  )}
                </div>
                <div className="shop-focus__section">
                  <h3>Modifiers</h3>
                  {selectedModifier ? (
                    <div className="shop-focus__stack">
                      <div>
                        {selectedModifierEmoji} {selectedModifier}
                      </div>
                    </div>
                  ) : (
                    <div className="shop-focus__muted">None active.</div>
                  )}
                </div>
              </div>
              <div className="shop-door" />
            </div>
          </div>
        </div>
      )}

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
