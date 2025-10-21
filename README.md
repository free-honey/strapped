# strapped smart contract

#### massively-multiplayer strip-craps game 😈

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


