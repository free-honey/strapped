# strapped smart contract

#### massively-multiplayer strip-craps game 😈

<img width="1727" height="1078" alt="image" src="https://github.com/user-attachments/assets/5ba11fd1-aaff-4625-9fe5-f0e43298b664" />


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

To get the best experience, place bets on Six and Eight only.

To populate the game and the shop, roll a seven. This should just require pressing `r` after starting up.


