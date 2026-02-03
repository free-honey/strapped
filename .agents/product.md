# Product design

## Vision
- 

## Target users
- 

## Core loops
- Liquidity pool for the house.
- Crowdfunding for modifiers.

## UX notes
- 

## Open questions
- 

# Strapped Epics

## Aardvark
- completed :)

## Binturong
### Features
#### Equipped state + suit-up flow + shop + party menu
- [x] Contract: Add equipped state per player with required slot(s).
- [ ] Contract: Add run-entry validation for required gear.
- [ ] Contract: Add suit-up / opt-in action.
- [ ] Contract: Add suit-up validation that checks required gear.
- [ ] Contract: Add a basic shop for level-1 straps (purchase + inventory update).
- [x] UI: Add a suit-up flow (select gear, confirm equipped state).
- [ ] UI: Add suit-up flow validation/error states for required gear.
- [ ] UI: Add a shop screen for level-1 gear (browse, buy).
- [ ] UI: Add a party menu showing your gear and party members' gear.
- [x] Indexer/API: Expose equipped state.
- [ ] Indexer/API: Expose shop items.
- [ ] Indexer/API: Expose inventory.
- [ ] Indexer/API: Expose party roster/gear.
- [ ] Tests: Add tests for equip validation and shop purchase; add a UI smoke test.

#### Run lifecycle + room generation
- [ ] Contract: Define the run lifecycle (start, active, end) and store run state.
- [ ] Contract: Generate a new run when the prior run ends.
- [ ] Contract: Derive room layout from VRF per game and allow dead-ends.
- [ ] Contract: Optionally fix room 1 to guarantee 2+ rooms.
- [ ] Contract: Add placeholders for run end checks (TBD list).
- [ ] Indexer/API: Expose run state, current room, and exits.
- [ ] UI: Show run status, current room, and next-room outcomes.
- [ ] Tests: Add tests that cover run lifecycle transitions and deterministic room generation.

#### Equipment effects (TBD)
- [ ] TBD.

#### UI style redesign
- [ ] UI: Add style for all different room types
- [ ] UI: Redesign straps?

### Open questions
- Run end conditions (primary check and fallback checks).
- Buy-in size and token type.
- Party pool design vs per-player pots/HP; whether donations are included at launch.
- Mid-run joining policy.
- Drop allocation weighting specifics and deterministic tie-breakers.
- Jackpot weighting formula and depth guarantees.
