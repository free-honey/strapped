# strapped smart contract

#### massively-multiplayer online strip-craps game 😈


Try the Web UI on testnet now:
https://strapped-ui.up.railway.app/

<img width="1696" height="960" alt="image" src="https://github.com/user-attachments/assets/a9fb8766-f771-4a4d-9064-c375e6b7994d" />


## Play locally

<img width="1728" height="1077" alt="image" src="https://github.com/user-attachments/assets/7545a9a1-a212-4810-b94c-225b0ab6af6c" />

### Forc

Check that you have the correct version of Forc installed:

```
? forc --version
forc 0.70.1
```

if not, please follow instructions here: https://docs.fuel.network/guides/installation/

## Quickstart (Testnet)

1. Set up a **named** wallet with forc-wallet
    - Import: `forc-wallet --path ~/.fuel/wallets/alice.wallet import`
    - New: `forc-wallet --path ~/.fuel/wallets/alice.wallet new`
1. Fund the wallet
    - Get the address: `forc wallet --path ~/.fuel/wallets/alice.wallet list`
    - Request funds: https://faucet-testnet.fuel.network/
1. Clone the repo
    - `git clone git@github.com:free-honey/strapped.git`
1. Run the TUI (new terminal, ideally fullscreen)
   ```
   cargo run -p tui -- --testnet --wallet alice --indexer-url https://strapped-indexer-test-net-production.up.railway.app
   ```
   - Enter your wallet password when prompted.
1. Play!
    - Move between the 11 rolls, place bets with `b`, manually roll with `r` to progress (until someone else is
      playing), claim rewards with `c`, bet clothing with `t`, and open the shop with `s` for modifiers. A roll of *
      *Seven**
      resets the game.
    - You can only claim rewards (`c`) after a game is over. Optionally apply modifiers to clothing before claiming (
      applied by default).

## Rules

Strapped is loosely based on the classic casino game Craps. The basic premise is you place bets on different rolls of
the dice. If you hit the target number with a roll, you win the bet! If a **Seven** is rolled, the house takes
everything, you
lose your outstanding bets, and the game resets. Your bets can be hit multiple times before the board is reset with a
seven.

The game is made more interesting by the addition of clothing items that can be bet. Win clothing by betting enough on
enough
on a roll space with clothing on it. Won clothing will be added to your inventory. That clothing can then be bet, but
instead
of earning more of that clothing, the clothing is upgraded to a higher level--higher level clothing therefore is rarer
and
more valuable.

There are also modifiers that can be applied your clothing. These modifiers can appear in the shop and once unlocked
with
with the correct rolls, can be bought and applied to a roll square. Clothing bets that "hit" on a square with modifiers
can optionally have the modifier applied to the clothing when upgraded.

## Developer helpers (xtask)

Handy tasks without doing a full `cargo clean` (alias available: `cargo xtask …`):

- Build Sway contracts + regenerate ABI: `cargo xtask abi`
- Just build the Sway projects: `cargo xtask build-sway`
- Clippy (warnings as errors): `cargo xtask clippy`
- Integration tests (builds Sway + ABI first): `cargo xtask test`
