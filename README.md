# strapped smart contract

#### massively-multiplayer online strip-craps game 😈

<img width="1728" height="1078" alt="image" src="https://github.com/user-attachments/assets/81e7d83a-5404-4aa6-a8e4-385de586ce05" />

# TUI

## Requirements

### Forc

Check that you have the correct version of Forc installed:

```
? forc --version
forc 0.70.1
```

### Forc-Wallet

You will need to set up Forc-Wallet to manage your wallets. The wallet needs to be named in your `.fuel/wallets` path.
You can create a wallet with name `alice` by running:

```
forc-wallet --path ~/.fuel/wallets/alice.wallet import
```

for existing wallet, or

```
forc-wallet --path ~/.fuel/wallets/alice.wallet new
```

## Running the TUI

To test out the game with a (vibe-coded) TUI, run e.g.:

```
cargo run -p tui -- --dev --wallet alice 
```

This will launch a tui that interacts with the latest deployed contract on the dev node (based on what is in the
.deployments/dev folder).

### Local Node with Fake VRF

or to have control over the VRF contract, use

```
cargo run -- --fake-vrf
```

> [!NOTE]
>
> Use `/` to modify the VRF number.
>
> `0` -> `Two`
>
> `10` -> `Six`
>
> `25` -> `Eight`
>
> and to end the current game:
>
> `19` -> `Seven`

From the base directory.

Many values in the Sway contract are currently hard-coded to enable simpler testing, but those will be improved over
time.

To populate the game and the shop, roll a seven. This should just require pressing `r` after starting up.

## Quickstart: indexer + TUI (devnet port-forward)

1. Set up a named wallet with forc-wallet  
   - Import: `forc-wallet --path ~/.fuel/wallets/alice.wallet import`  
   - New: `forc-wallet --path ~/.fuel/wallets/alice.wallet new`
2. Fund the wallet  
   - Get the address: `forc wallet --path ~/.fuel/wallets/alice.wallet list`  
   - Request funds: https://faucet-devnet.fuel.network/
3. Clone the repo  
   - `git clone git@github.com:free-honey/strapped.git`
4. Port-forward the o2 sentry (new terminal)  
   - `aws eks update-kubeconfig --name fuel-dev-2-hybrid --region us-east-1`  
   - `kubectl port-forward -n devnet-0 pod/fuelcore-sentry-o2-0 4000:4000`
5. Run the indexer (new terminal)  
   - `cargo run -p indexer -- --graphql-url http://127.0.0.1:4000/graphql --port 5000 --tracing --dev`  
   - Uses `.deployments/dev` to choose the contract id and start height; let it finish indexing before starting the TUI.
6. Run the TUI (new terminal, ideally fullscreen)  
   - `cargo run -p tui -- --devnet --wallet alice --indexer-url http://127.0.0.1:5000`  
   - Enter your wallet password when prompted.
7. Play!  
   - Move between the 11 rolls, place bets with `b`, manually roll with `r` to progress (until someone else is playing), claim rewards with `c`, bet clothing with `t`, and open the shop with `s` for modifiers. A roll of Seven resets the game.

## Developer helpers (xtask)

Handy tasks without doing a full `cargo clean` (alias available: `cargo xtask …`):

- Build Sway contracts + regenerate ABI: `cargo xtask abi`
- Just build the Sway projects: `cargo xtask build-sway`
- Clippy (warnings as errors): `cargo xtask clippy`
- Integration tests (builds Sway + ABI first): `cargo xtask integration-tests`
