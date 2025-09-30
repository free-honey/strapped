# strapped smart contract

#### massively-multiplayer strip-craps game 😈

<img width="1728" height="1078" alt="image" src="https://github.com/user-attachments/assets/81e7d83a-5404-4aa6-a8e4-385de586ce05" />



To test out the game with a (vibe-coded) TUI, run:
```
cargo run
```
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

Many values in the Sway contract are currently hard-coded to enable simpler testing, but those will be improved over time. 

To populate the game and the shop, roll a seven. This should just require pressing `r` after starting up.


