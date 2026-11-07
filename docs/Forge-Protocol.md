# Forge protocol

Neo Forge 3D uses the rules engine of **Forge** (Java), in two ways with the same protocol:

**Standalone:** Godot starts Forge as a separate process and talks to it over stdin/stdout.

```
Godot (Rust)                                  Java process (forge-bridge/)
ForgeBridge node (clash-forge-client)         clash.forgebridge.app.BridgeMain
          ── JSON line on stdin ──▶             → ClashBridge → engine.BridgeCore
  signals ◀── JSON line on stdout ──             └─ Forge (forge-game, forge-ai, …)
                       stderr ──▶ user://forge-bridge.log
```

Other front ends and hosts embed the same building blocks – see [Embedding.md](Embedding.md).

**Embedded in NeoForge:** NeoForge (JavaFX front end for Forge) starts the match and Godot as the
game table. Godot connects over a local socket.

```
NeoForge (JVM)                                              Godot (started by NeoForge)
NeoGame.play ── ClashTable.open() → TableLauncher.launch(): open port, start Godot ──▶ godot --path … -- --clash-port=N
   │            ClashBridge.attach(Socket)  ◀── JSON lines over TCP 127.0.0.1:N ──▶ ForgeBridge node
   └─ HostedMatch: human seat = ClashBridge.remotePlayer(…) (RemotePlayerController)
```

- NeoForge side (shipped separately, `integrations/neoforge/`): `forge.neo.clash.ClashTable` and
  the patch `neoforge-host.patch` (changes to `NeoGame`, `SafeAi`, `NeoMain`, `SettingsPanel`,
  language files). `tools/sync_neoforge.sh` copies the bridge (without `app/`) and `ClashTable`
  to `forge-gui-neo/src/main/java/`; the version in this repository is authoritative.
- Enabled in NeoForge under *Settings → Table → 3D table*, or via `neo.clash=true` (or
  environment `NEOFORGE_CLASH=true`); Godot executable `neo.clash.godot` / `CLASH_GODOT`, project
  folder `neo.clash.project` / `CLASH_PROJECT` (system property → NeoForge setting → environment).
  Only matches with exactly two players and without Planechase/Archenemy/Vanguard or hot seat
  (`ClashTable.useFor`); all others stay on NeoForge's table.
  Test command: `NeoMain clash [--deck=NAME] [--opponents=N] [--timeout=SEC]` or `tools/neoforge_clash.sh`.
- Flow: `ready` (with `embedded: true`) → `match_started` (without state; the state comes with
  the first request) → requests as usual → `game_over` (from `GameEventGameOutcome`) → NeoForge
  closes the socket, Godot exits. `start_match` is not allowed when embedded.
- If the player closes the Godot window during the match, their seat concedes.
- NeoMatchUI runs invisibly as a spectator (like NeoForge's OBSERVE mode without a table);
  hot-seat rivals are played by the AI when the 3D table is used.

## License

Forge is licensed under the **GPL-3.0**; Neo Forge 3D adopts this license for the whole project
(`LICENSE`, same text as `forge-bridge/LICENSE`). Forge's program jar and card database
(`res/cardsfolder`, contains cards by Wizards of the Coast) are not in the repository; they are
read from the local Forge installation.

## Setup

1. Install Forge (desktop version), e.g. to `~/Games/Forge`. You need
   `forge-gui-desktop-*-jar-with-dependencies.jar` and the `res/` folder.
   **Project copy:** if the jar and `res/` are in `forge-bridge/forge/`, this copy is preferred
   (by `build.sh`, `run.sh` and `ForgeBridge`). The folder is excluded via `.gitignore`
   (GPL code and Wizards card data, about 160 MB). Create it from an installation:
   ```sh
   mkdir -p forge-bridge/forge
   rsync -a ~/Games/Forge/forge-gui-desktop-*-jar-with-dependencies.jar ~/Games/Forge/LICENSE.txt forge-bridge/forge/
   rsync -a --exclude adventure --exclude music --exclude 'languages/*' ~/Games/Forge/res forge-bridge/forge/
   cp ~/Games/Forge/res/languages/{en-US,de-DE}.properties ~/Games/Forge/res/languages/cardnames-de-DE.txt forge-bridge/forge/res/languages/
   ```
   Adventure mode, music and other language files are left out – they are not needed for
   matches through the bridge.
2. Java 17 or newer (`java`, `javac`, `jar` on the PATH).
3. Build the bridge: `forge-bridge/build.sh` → `forge-bridge/build/forge-bridge.jar` (program) and
   `forge-bridge-core.jar` (library for embedding), about 2–5 s.
   Different Forge path: `FORGE_JAR=/path/to/forge.jar forge-bridge/build.sh`.
4. In Godot, add a `ForgeBridge` node and set `forge_dir` / `forge_jar` if needed
   (empty = `FORGE_DIR`, otherwise `forge-bridge/forge/`, otherwise `~/Games/Forge`; the jar is
   found automatically).

Manual test without Godot: start `forge-bridge/run.sh` and type JSON lines, e.g.
`{"cmd":"ping"}`.

**Card data by name:** `{"cmd":"card_info","names":["Lightning Bolt", …]}` → `{"type":"card_info","cards":[{name, type, cost, text, power?, toughness?, image?}, …]}` (unknown names are missing; `image` is missing while the image is still being downloaded – ask again later). Used for the search when naming a card.

## What the engine decides and what the player decides

Since protocol 3 the bridge speaks the **protocol model** (`Decision` / `Choice`, modelled on
`forge-engine/src/decision.rs` from forge-rules-rs, a separate Rust rewrite of Forge's rules – not
NeoForge). The Java bridge translates Forge's `PlayerController` calls into `Decision`s
(`engine/Decisions.java`, `engine/RemotePlayerController.java`).

| Forge method(s) | `Decision` | Answer (`Choice`) |
|---|---|---|
| `chooseSpellAbilityToPlay` | `Priority { actions, elsewhere }` (`elsewhere`: playable cards outside hand and battlefield with their zone, see extensions); without a playable action the bridge passes by itself, except in the player's own second main phase and end step with an empty stack (then `Priority` with only `Pass`, so the player stops there) (additionally `ActivateMana(card, i)` per mana ability as soon as there is something to play, and `UndoMana(card)` per card whose mana ability Forge can still undo) | `Action(…)`; `Pass`; `Action(ActivateMana)` → run the ability immediately (mana into the pool), ask again; `Action(UndoMana)` → `MagicStack.filterUndoStackByHost` + `sa.undo()` (card untaps, mana leaves the pool), ask again |
| `mulliganKeepHand` | `Mulligan { mulligans_taken, first_player }` | `Keep` / `Mulligan` |
| `chooseModeForAbility` | `ChooseModes { modes, min, max, can_repeat }`; for modes with their own additional costs (Spree, Tiered: parameter `ModeCost`) additionally `costs` (per mode, e.g. `["{1}", "{3}{W}{W}"]`), `base_cost` (`"{W}"`), `keyword` (`"Spree"` / `"Tiered"`) | `Modes([…])` |
| `chooseTargetsFor` (via `setupTargets`) | `ChooseTargets { source, description, candidates, min, max }` | `Targets([…])` / `Cancel` |
| `announceRequirements` (X from 0) | `ChooseX { max }` (UI: 3D buttons at the stack, `ForgeTable.show_choose_x`) | `Number(n)` / `Cancel` |
| `chooseColor`, `chooseColors` with exactly one colour | `ChooseColor { options }` | `Color("R")` |
| `chooseCardName` | `NameCard { options, suggested }` (all allowed names; `suggested` = indices of names the player knows: own cards in all zones and face-up cards of opponents on the battlefield, in graveyard, exile, stack and command zone) | `Number(index)` |
| `chooseSomeType` | `ChooseType { options, kind, suggested }` (`kind` = Forge's kind of type, e.g. `"Creature"`, `"Basic Land"`, `"Card"`; `suggested` = indices of types that appear on the player's own cards in all zones, including the library) | `Number(index)` |
| `confirmAction`, `confirmReplacementEffect`, `confirmStaticApplication` | `Confirm { prompt }` | `Yes` / `No` |
| `confirmTrigger` | `OptionalTrigger { description }` | `Yes` / `No` |
| Starting player, vote, counter type, protection, keyword, sector, dice, number from a list, yes/no with custom labels (heads/tails, tap/untap …), colour including colourless | `ChooseOption` (**extension**) | `Number(index)` |
| `chooseSingleReplacementEffect` (CR 616.1) | `ChooseOption { options: [{text, card}], purpose: "ReplacementEffect" }`: per replacement effect its text and the card ID of its source, whose data is in the envelope's `cards` | `Number(index)` |
| `chooseOptionalCosts`, `chooseColors` (several), `chooseDiceToReroll` | `ChooseOptions` (**extension**) | `Modes([…])` |
| `addKeywordCost` (keyword additional costs when casting, CR 601.2b: Casualty, Conspire, Offspring, tapping for Harmonize) | `ChooseOptions { options: ["Casualty"] (keyword without number; pure mana costs with amount, e.g. "Offspring {2}"), min: 0, max: 1, purpose: "OptionalCost" }` (like kicker; without the override Forge asked via `chooseNumberForKeywordCost` with maximum 1) | `Modes([0])` = pay, `Modes([])` = normal |
| `Cancel` on `OptionalCost` (cancelable while casting through the human flow) | `chooseOptionalCosts` (kicker …, asked before the spell goes onto the stack): the cast ends at once (nothing has changed). `addKeywordCost` (casualty, offspring …, asked while casting): the bridge refuses the next target choice or cost payment, so Forge rolls the cast back completely. Either way `rejected` follows and the player gets priority again |
| `chooseNumber`, `chooseNumberForCostReduction`, `chooseNumberForKeywordCost` (Multikicker, Replicate, Squad, "pay any amount of mana"), announced values ≠ X | `ChooseNumber` (**extension**) | `Number(n)` |
| `declareAttackers` | `DeclareAttackers { candidates, defenders }` | `Attackers([[card, target], …])`; empty = no attack |
| `declareBlockers` | `DeclareBlockers { candidates, attackers }` | `Blocks([[blocker, attacker], …])`; empty = no blocks |
| `applyManaToCost` (via `payManaCost`) | `PayMana { remaining, options: [ActivateMana(card, i), AutoPay, Cancel] }` | `Pay(ActivateMana(…))` → tap the source, apply the mana, ask again until paid; `Pay(AutoPay)`; `Pay(Cancel)` = undo the cast |
| `chooseCardsForEffect`, `chooseCardsForZoneChange`, `chooseSingleCardForZoneChange`, `chooseCardsToDiscardFrom`, `chooseCardsToDiscardToMaximumHandSize`, `choosePermanentsToSacrifice`, `choosePermanentsToDestroy`, `chooseCardsToRevealFromHand`, `chooseSingleEntityForEffect` / `chooseEntitiesForEffect` (cards only) | `ChooseCards { reason, options, min, max }` (`reason`: `Choose`, `Search`, `Discard`, `DiscardToHandSize`, `Sacrifice`) | `Cards([…])` |
| `chooseSingleEntityForEffect` / `chooseEntitiesForEffect` with players (e.g. "Choose a player") | `ChooseTargets { source, description, candidates, min, max, purpose: "Choose" }` (**extension**: not a target (CR 115.1), the UI shows buttons per player; the base model has no player choice for effects; `source` = −1 if there is no source) | `Targets([…])` |
| `chooseNewTargetsFor` (redirecting, copies with "you may choose new targets") | for an optional change first `Confirm` ("Choose new targets for …?"), then `ChooseTargets` (with Forge's additional filter) | `Yes`/`No`, `Targets([…])` |
| `exertAttackers`, `enlistAttackers` (CR 701.43, 702.154) | `ChooseCards { reason: "Choose", options: attackers, min: 0 }` (table mode) | `Cards([…])` |
| `chooseSingleSpellForEffect` / `chooseSpellAbilitiesForEffect` | `ChooseOption` (`purpose: "SpellForEffect"`) / `ChooseOptions` (`purpose: "SpellsForEffect"`, `min` = `max` = count) with `options[].card` = card of the ability, text without the card name | `Number` / `Modes` |
| Venture: `chooseSingleCardFace` with dungeons / `chooseSingleSpellForEffect` with rooms | `ChooseOption { purpose: "Dungeon" }` (per dungeon card data with `dungeon` and `image` – Scryfall image of the token printing – in the envelope's `cards`) or `ChooseOption { purpose: "DungeonRoom" }` (`options[].key` = room name, `options[].card` = the dungeon) | `Number` |
| `chooseSingleStaticAbility` | `ChooseOption { purpose: "StaticAbility" }` with `options[].card` = source card | `Number` |
| `chooseSingleCardFace` / `chooseSingleCardState` | `ChooseOption { purpose: "CardFace" }`, separate card data per face in the envelope's `cards` (IDs from 1 900 000 000, not in the game) | `Number` |
| `orderAndPlaySimultaneousSa` with copies | copy onto the stack like Forge's human controller; new targets via `chooseNewTargetsFor` | – |
| `playSaFromPlayEffect` (cascade, "you may cast", miracle …) | via Forge's human flow (the player is asked for targets, modes, costs) | – |
| `payCombatCost` (attack/block costs, CR 508.1h) | `Confirm`, then payment as when casting | `Yes`/`No` |
| `confirmPayment` | `Confirm` | `Yes`/`No` |
| `chooseCardsPile` (Fact or Fiction etc.) | `ChooseOption { purpose: "Piles" }`, per pile `cards` (IDs of the face-up cards, data in the envelope's `cards`) or `hidden` (number of face-down cards); Forge's parameter `FaceDown` ("False", "One" = pile 1 face down, "True") | `Number` |
| `orderCosts` | `OrderTriggers` with cost texts | `Order` |
| `chooseCardsForCost` (costs during resolution) | `ChooseCards` with a reason per cost type (`DiscardCost`, `SacrificeCost`, `ExileCost`, `TapCost`, `ReturnCost`, otherwise `Choose`) | `Cards` |
| `chooseCardsToDiscardUnlessType` | `ChooseCards { reason: "Discard", min: 1, max: n }`: n cards or one card of the type | `Cards` |
| `chooseSaToActivateFromOpeningHand` (Leylines …) | `ChooseOptions { purpose: "OpeningHand", min: 0 }` with `options[].card` | `Modes` |
| `getAbilityToPlay` | `ChooseOption { purpose: "AbilityToPlay" }`, every option with `card` = the card, cancelable | `Number` |
| `chooseManaFromPool` (several matching mana in the pool) | `ChooseOption` with mana and source (**extension**: the base model (`PayAction`) has no pool payment) | `Number` |
| `chooseCardsForSplice` (CR 702.47) | `ChooseCards { reason: "Choose", min: 0 }` from the hand | `Cards` |
| `chooseCardsForEffectMultiple` | one `ChooseCards { min: 0/1, max: 1 }` per group | `Cards` |
| `chooseTarget` (which target is changed) | `ChooseOption` "spell → target", `purpose: "Retarget"` | `Number` |
| `reveal` (cards are revealed, CR 701.16) | `ChooseCards { reason: "Reveal", options, min: 0, max: 0 }` (**extension**: display only; "Done" = seen); the player's own hand cards are not shown | `Cards([])` |
| `notifyOfValue` (e.g. "chooses red") | message `log` with `kind: "INFORMATION"` (no decision) | – |
| `divideShield` (prevention shield over several affected targets, CR 615.7) | `DivideAmount { amount: shield amount (at most total damage), targets, min_each: 0 }` | `Damage([…])` |
| `chooseRollToSwap`, `chooseRollSwapValue` (swap a die roll) | `ChooseOption`, `purpose: "Dice"` | `Number` |
| `specifyManaCombo` ("n mana in any combination of colours") | one `ChooseColor` per mana | `Color` |
| `playSpellAbilityNoStack` | via Forge's human flow | – |
| `payCostDuringRoll` (costs to change a roll) | `Confirm`, then payment as when casting | `Yes`/`No` |
| Out of scope (stay with the AI) | Ante, Contraptions, Planechase die, Assist (multiplayer), variant starting hand, sideboard, bids | – |
| `chooseCardsToDelve` (CR 702.66) | `ChooseCards { reason: "Delve", options: graveyard, min: 0, max }` | `Cards([…])` |
| `chooseCardsForConvokeOrImprovise` (CR 702.51, 702.126, Waterbend) | `ChooseCards { reason: "Convoke" / "Improvise" / "Choose", options: untapped creatures/artifacts, min: 0, max }`; the bridge decides which mana each card pays (coloured symbol in one of the creature's colours, otherwise generic) | `Cards([…])` |
| `orderBlockers`, `orderBlocker`, `orderAttackers` (damage assignment order, only with `order_combatants`) | `OrderTriggers { descriptions }` with card names (like `orderMoveToZoneList`) | `Order([…])` |
| `arrangeForScry`, `arrangeForSurveil` | `Scry { cards, graveyard }` | `Arrange { top, bottom }` |
| `orderSimultaneousSa` (own triggers), `orderMoveToZoneList` | `OrderTriggers { descriptions, sources }` (`sources` = card ID of the source per entry, −1 = none) | `Order([…])` (first one first) |
| `tuckCardsViaMulligan` | `MulliganBottom { hand, count }` | `Cards([…])` |
| Costs: sacrifice, discard, tap, return, exile (`getCostDecisionMaker` → `RemoteCostDecision`) | `ChooseCards { reason: SacrificeCost / DiscardCost / TapCost / ReturnCost / ExileCost }`; special exile cases (the card itself, all, top graveyard cards, minimum mana value, library, "from the same graveyard") are decided by the engine; crewing vehicles (Crew, total power ≥ N) as `TapCost` with `min: 1`, too little power → ask again | `Cards([…])` |
| `assignCombatDamage` | `AssignCombatDamage { attacker, amount, blockers, lethal, trample_to }` | `Damage([per blocker…, trample])` |
| `chooseTargetsFor` with "DividedAsYouChoose" | `DivideAmount { source, amount, targets, min_each }` (**extension**) | `Damage([…])` |
| `payCostToPreventEffect` | `PayUnlessCost { cost }` (only if payable) | `Yes` / `No` |
| Phyrexian mana in `applyManaToCost` | `PayLifeForPhyrexian { symbol }` | `Yes` / `No` |
| `orderAndPlaySimultaneousSa`, `playTrigger` (own triggers) | via `PlaySpellAbility.playSpellAbility` or `playSpellAbilityNoStack` (like Forge's human controller): the player is asked for modes, targets, costs | – |
| Opponents (seats that are not remote-controlled) | – | Forge AI |

**Flow when playing** (`RemotePlayerController.playChosenSpellAbility`): the bridge calls Forge's
own flow for human players, `forge.game.player.PlaySpellAbility.playSpellAbility` (contained in
the jar). It asks the controller in order: additional/optional costs (`chooseOptionalCosts`) →
modes (`chooseModeForAbility`) → announced type → X value (`announceRequirements`) → targets
(`setupTargets` → `chooseTargetsFor`) → costs (`CostPayment` with `getCostDecisionMaker`; mana via
`payManaCost` → `PlaySpellAbility.payManaCost` → `applyManaToCost` = `PayMana`). Cancelling at any
point → Forge undoes the cast, the bridge reports `rejected`. Freely divided damage is asked by
`chooseTargetsFor` after target selection as `DivideAmount` (Forge's GUI also does this during
target selection, `addDividedAllocation`).

**Paying mana** (`applyManaToCost`): floating mana in the pool is only spent via `UsePool` or
`AutoPay`. `PayMana` offers all mana abilities of the player's own permanents that can be
activated now. `ActivateMana`: pay the mana ability's cost (tapping; decided by
`AiCostDecision`), resolve the ability, apply the mana with `ManaPool.payManaFromAbility` – like
Forge's AI for a single source. `AutoPay`: `ComputerUtilMana.payManaCost`.

**Combat** (`RemotePlayerController.declareAttackers` / `declareBlockers`): candidates are the
creatures for which Forge allows `CombatUtil.canAttack` or `canBlock`; every answer is checked with
`canAttack(card, target)` / `canBlock(attacker, blocker)` and then `CombatUtil.validateAttackers`
or `validateBlocks` (attack requirements, "can't be blocked except by two or more creatures" …).
Invalid declarations are undone and asked again with a hint in the `prompt` (at most 4 times,
then the AI decides).

**Priority actions**: land → `PlayLand(card)` (further land faces of the same card `PlayLandAlt`),
spell → `Cast(card)` (further spell variants of the same card `CastAlt(card, n)`), activated
ability → `Activate(card, n)` (n = running index of the card's offered abilities). Only what is
playable and payable right now and has possible targets is offered. If the chosen action cannot
be performed (cancelled, no valid target), `rejected` follows together with a new `Priority`
request. Whether the Forge AI considers the move sensible does not matter.

### Extensions to the base model

These parts do not (yet) exist in the model (`decision.rs` from forge-rules-rs):

| Extension | Purpose |
|---|---|
| `Decision::ChooseOption { player, prompt, options: [{text, color?}] }` → `Choice::Number(index)` | general single choice (see the table above); `color` = `W U B R G` or `C` (colourless) |
| `Decision::ChooseOptions { player, prompt, options, min, max }` → `Choice::Modes(indices)` | general multiple choice |
| `Decision::ChooseNumber { player, prompt, min, max }` → `Choice::Number(n)` | number with a lower bound ≠ 0 or outside of X |
| `Decision::DivideAmount { player, source, amount, targets, min_each }` → `Choice::Damage(amounts)` | divide an amount among the chosen targets while casting (CR 601.2d) |
| Targets on the stack in `ChooseTargets` | as `{"card": id}` of the source of the spell or ability (the base model's `Target` has no stack entries; to add: `Target::Stack(id)`) |
| `ChooseOption` for mana from the pool (`chooseManaFromPool`) | the base model lacks `PayAction::UsePool(colour)` |
| `Action::UndoMana { card }` in `Priority.actions` | undo a previously activated mana ability of the card as long as its mana has not been spent (Forge's undo stack for mana abilities); the base model has no undo |
| `OrderTriggers.sources` | card ID of the source per entry (−1 = none), so the UI shows card copies instead of texts |
| `Action::ActivateMana { card, index }` in `Priority.actions` | activate a mana ability with priority (CR 605.3a), e.g. tap lands in advance; `index` as in `PayAction::ActivateMana`. The base model knows mana abilities only while paying |
| `ChooseTargets.purpose` (`"Choose"` for `chooseSingleEntityForEffect` / `chooseEntitiesForEffect`) | choice for an effect without a target ("choose a player", CR 115.1); empty = a real target of a spell or ability (CR 115), which the UI only lets you choose with the arrow |
| `Priority.elsewhere` (`[{card, zone, cost}]`, `cost` = mana cost of the way to play it from that zone, e.g. flashback, card data in the envelope's `cards`) | cards that are currently playable from a zone other than hand and battlefield (graveyard, exile, command zone, sideboard/companion, library; `zone` = `Prepared` for the spell copy of a prepared creature, which internally lies in exile – image = the printed creature card); the UI additionally shows them in the hand fan |
| `ChooseOption.purpose`, `ChooseOptions.purpose`, `options[].card`, `options[].key` | what the choice is for (`"ReplacementEffect"`, `"StaticAbility"`, `"CardFace"`, `"SpellForEffect"`, `"SpellsForEffect"`, `"AbilityToPlay"`, `"CounterType"`, `"Keyword"`, `"Protection"` – colours with `color`, then the colour wheel –, `"NumberList"`, `"Vote"`, `"ManaPool"` for `chooseManaFromPool`, `"Dice"` for the die-roll choices `chooseRollToIgnore` / `chooseRollToModify` / `chooseRollToSwap` / `chooseRollSwapValue` / `chooseDiceToReroll`, `"Retarget"` for `chooseTarget`); the UI derives its stage heading from `purpose` (only the mechanic, never card names or texts); `key` = engine key of an option (counter type, e.g. `P1P1`) and which card each option belongs to; with a card per option the UI shows a card copy per possibility instead of buttons |
| Stack entry `modes` | texts of the chosen modes of a modal spell (Forge attaches them as sub-abilities with SVar `CharmOrder`) |
| `ChooseModes.costs`, `base_cost`, `keyword` | modes with their own additional costs (Spree CR 702.172, Tiered): cost per mode, cost without modes and the keyword, so the UI can show "{cost}" above each mode and "Pay {total cost}"; without mode costs the fields are absent |
| `ChooseOptions.purpose` (`"OptionalCost"` for `chooseOptionalCosts` and `addKeywordCost`) | what the multiple choice is for, so the UI shows optional costs (kicker …) on their own stage; otherwise empty |
| `PayAction::UsePool { color }` (`W U B R G C`) in `PayMana.options` | spend floating mana of one colour; the bridge no longer spends pool mana automatically (only via `UsePool` or `AutoPay`) |
| `ChooseCards` with `reason: "Reveal"` and `max: 0` | display of revealed cards; the base model lacks a `Decision::Reveal { cards }` or event |
| `Decision::ChooseStartingPlayer { player, players }` → `Choice::Number(index)` | who starts the game (CR 103.1); `players` = seats, `labels` = player names. The UI shows 3D buttons for it instead of a window |
| `Decision::Mulligan` + field `first_player` | seat of the starting player (−1 = not known yet), so the mulligan stage can show "You go first" / "X goes first"; the base model only has `mulligans_taken` |
| `ChooseTargets` also for player/card choices by effects (`chooseSingleEntityForEffect` with players) | the base model has no decision for this; the UI treats them like targets (click on medallion/card) |
| `NameCard.suggested`, `ChooseType.kind`, `ChooseType.suggested` | suggestion list for naming a card / choosing a type (indices into `options`) and the kind of type, so the UI can show "Choose a creature type." with the types of the own deck first ("Show all" for the rest) |
| `Choice::Cancel` | cancel the target/X choice while casting (the cast is undone); for other decisions "the engine decides by itself" |
| Request envelope: `prompt`, `source`, `ability`, `cancelable`, `labels`, `cards` | display texts (Forge's question text, source card, ability text, display text per list entry) and card data (`cards`: list as in `state`) of the cards concerned – also from hidden zones, e.g. for `ChooseCards` with `Search` |

## Protocol (version 3)

JSON Lines, UTF-8: exactly one JSON message per line.

### Commands (Godot → Forge, stdin)

| `cmd` | Fields | Effect |
|---|---|---|
| `ping` | `nonce` (optional) | answer `pong` with the same `nonce` |
| `start_match` | `decks`: list of `.dck` paths or Forge deck names (at least 2) · `remote`: list of bools per seat (true = controlled by Neo Forge 3D) · `names`: player names · `seed` (optional) · `starting_life` (optional) · `game_type` (optional, `"Commander"` = Commander game, decks with a `[Commander]` section, starting life 40) · `ai_pace` (optional, number, default 1: factor for the Forge AI's pauses after its actions – land 0.7 s, spell/ability 1 s, attackers 0.9 s, blockers 0.7 s, phase change during the AI's turn 0.25 s; 0 = no pauses; `AiPace.java`) · `order_combatants` (optional, bool: Forge's old rule "order combatants", damage assignment order before the 2024 CR change) | starts a match (constructed, 1 game) |
| `answer` | `id`: request ID · `choice`: answer | answers a `request` |
| `state` | `viewer`: seat whose hand is visible (-1 = none) | answer `state` |
| `card_info` | `names`: list of card names | answer `card_info` (see "Setup") |
| `debug_card_names` | – | debug window: answer `debug_card_names` with all card names |
| `debug_add_card` | `seat`, `name` | debug: create a new card and put it into the player's hand |
| `debug_infinite_mana` | `seat`, `on` | debug: refill the player's mana pool to 50 per mana type (WUBRGC) on every phase change and spell, or turn that off |
| `quit` | – | ends the process |

### Messages (Forge → Godot, stdout)

| `type` | Fields |
|---|---|
| `ready` | `protocol`, `embedded` (true = embedded in NeoForge), `working_dir` – Forge is loaded (about 5 s after start) |
| `pong` | `nonce` |
| `match_started` | `state` |
| `event` | `event` (Forge class name, e.g. `GameEventCardChangeZone`, `GameEventTurnPhase`), `text`, `turn`, `phase` (before the first turn – e.g. mana shards an Adventure duel hands out at the start – `turn` 0 and no `phase`); additional fields: `GameEventCardTapped` → `card`, `tapped`; `GameEventManaPool` → `player`, `mode` (`Added` …), `colors` (WUBRGC); `GameEventCardDamaged` → `card`, `source`, `amount`; `GameEventPlayerDamaged` → `player`, `source`, `amount`, `combat` (for the UI's mana and combat effects) |
| `log` | new entry in the game log (Forge's `GameLog`, chronological): `kind` (`GameLogEntryType`, e.g. `TURN`, `LAND`, `STACK_ADD`, `STACK_RESOLVE`, `ZONE_CHANGE`, `DAMAGE`, `LIFE`, `COMBAT`, `PHASE`, `MANA`, `MULLIGAN`), `text` (English, objects as "Name (id)"), `card` (card ID of the source, if known), `turn`. Godot signal `ForgeBridge.log_entry` |
| `request` | `id`, `player`, `decision` (`Decision`), `prompt`, `source` `{id, name}`, `ability`, `cancelable`, `labels`, `state` – see below |
| `rejected` | `player`, `option` (index in `actions` without `Pass`), `action`, `reason` – the chosen action cannot be performed |
| `state` | `state` |
| `card_info` | `cards` |
| `game_over` | `winner` (name or null), `draw`, `reason` |
| `debug_card_names` | `names`: all card names of the card database, sorted |
| `chat` | `from`, `text` |
| `error` | `code` (`init_failed`, `bad_command`, `unknown_command`, `deck_not_found`, `match_running`, `no_match`, `unknown_request`, `game_crashed`, `unknown_card`, `bad_seat`, `debug_failed`), `message` |

The game thread waits at every `request` until the matching `answer` arrives. **Debug commands**
(`debug_add_card`, `debug_infinite_mana`) change the game on the game thread while it waits like
this (not before – while the AI is thinking, they wait). Afterwards the bridge sends `state`; it
answers an open `Priority` request itself with `{"type":"Refresh"}` and asks it again (with the
new cards or the new mana) – so the UI gets a new `request` with a new `id`.

**`request`** (protocol v3)

```json
{"type": "request", "id": 7, "player": 0,
 "decision": {"type": "ChooseTargets", "player": 0, "source": 34, "description": "Choose target: Select any target",
              "candidates": [{"player": 0}, {"player": 1}, {"card": 83}], "min": 1, "max": 1},
 "prompt": "Choose target: Select any target", "source": {"id": 34, "name": "Lightning Bolt"},
 "ability": "Lightning Bolt deals 3 damage to any target.", "cancelable": true,
 "labels": ["Player (you)", "Forge AI", "Llanowar Elves (Forge AI)"], "state": {…}}
```

JSON form of the types: variant as `"type"`, fields in `snake_case`; `Target` =
`{"card": id}` / `{"player": seat}`; `Action` = `{"type": "Cast", "card": 34}` or with `"index"`
(`CastAlt`, `PlayLandAlt`, `Activate`, `CastWithAltCost`); `Color` = `"W" "U" "B" "R" "G"`;
`ChooseModes.modes` = list of `[index, text]`. `labels` = display text per entry of the main list
(`actions` for Priority, `candidates` for ChooseTargets, `modes` for ChooseModes).

**Answer** `{"cmd": "answer", "id": 7, "choice": {…}}` with a `Choice`:
`{"type": "Keep"}`, `{"type": "Mulligan"}`, `{"type": "Action", "action": {…}}`,
`{"type": "Targets", "targets": [{"card": 83}]}`, `{"type": "Number", "value": 2}`,
`{"type": "Yes"}` / `{"type": "No"}`, `{"type": "Color", "color": "R"}`,
`{"type": "Modes", "modes": [0, 2]}`, `{"type": "Cancel"}` (extension), and also `Pay`,
`Attackers`, `Blocks`, `Damage`, `Cards`, `Order`, `Arrange`. Unsuitable or invalid answers
count as cancelling; where that is not allowed, the Forge AI decides. A single choice with only
one option and no cancel is not asked at all.

**`state`**

```json
{
  "turn": 3, "phase": "MAIN1", "active_player": 0,
  "players": [
    { "seat": 0, "name": "Player", "life": 20, "library": 50, "hand_count": 5,
      "hand": [ …only for the viewer… ],
      "battlefield": [ { "id": 12, "name": "Mountain", "type": "Basic Land - Mountain", "tapped": false } ],
      "graveyard": [], "exile": [] }
  ]
}
```

Cards: `id`, `name`, `type`, `tapped`, `cost`, `keywords` (list of `{title, reminder}`: keywords
with the engine's reminder text, for the card view), `colors` (colours as `WUBRG` letters, empty =
colourless), `text` (rules text with current changes, symbols in `{…}` notation); for creatures
additionally `power`, `toughness` (current) and `base_power`, `base_toughness` (printed), for
planeswalkers `loyalty`, for battles `defense` (in play the current counters, otherwise printed),
`damage` (only > 0, damage taken); only when applicable: `token: true`, `attached_to` (card ID of
the host for auras, equipment, fortifications), `commander: true` (with `commander_tax` = the commander tax
in generic mana, CR 903.8: {2} per previous cast from the command zone, and – while in the command
zone – `cast_cost` = the mana cost to cast it from there including the tax, e.g. `"4 {R} {R}"`),
`emblem: true`, `room` (current
room of a dungeon), `sick` (creature is summoning sick, CR 302.6), `attacking` (attacking in the
current combat), `class_level` (classes, CR 716), `dungeon` (dungeons, CR 309; image via their
token printing like tokens: `{rooms: [{name, text, next: [room names]}], current}` from the Oracle
text, first room = entrance), `loyalty_abilities` (planeswalkers on the battlefield: all loyalty
abilities `[{label, cost, text}]`, including ones that cannot be activated right now; `label`
matches the label in `Priority.labels`, `cost` e.g. "+1", "-3", "0"), `prepared` (the creature is
prepared – Prepare: a copy of its spell can be cast), `unlocked_doors` (rooms: names of the
unlocked doors), `image` (absolute path of the card image, a Scryfall JPG in Forge's image folder
`pics/cards/<SET>/<Name>.fullborder.jpg` or in NeoForge's image folder; if it is missing, the
bridge downloads it from Scryfall in the background – `CardImages.java` – and on its own sends a
`state` message with the paths after the last image of a batch is done; never for face-down cards
and tokens; for the image the bridge picks a printing with a normal frame, see DOCUMENTATION.md),
`crop` (path of the image crop, always – for the tiles on the battlefield), `art` (only for cards
without a normal-frame printing: path of the image crop `<SET>/<Name>.artcrop.jpg`, Scryfall
`art_crop`), `face_down: true` (face down, CR 708: empty name, values of the face-down side),
`face` (for double-faced, modal and flip cards: 0 = front, 1 = back/flipped) with `other_face`
`{name, type}` (only face up), `counters` (counters like `CardObj.counters` in forge-rules-rs:
name → count, names = Forge's `CounterEnumType`, e.g. `{"P1P1": 2, "LOYALTY": 4}`).

Players additionally: `mana_pool` = amount of floating mana per colour in the order W U B R G C;
`counters` = player counters (CR 122.1) as for cards, e.g. `{"POISON": 3, "ENERGY": 2}`;
`command` = cards in the command zone (CR 408: commander, emblems, dungeons, "The Ring", "The
Monarch"/"The Initiative" and other effect cards; without Forge's internal "Commander Effect" card,
which only carries the commander rules); `blessing` (city's blessing), `ring_level` (how
often the Ring has tempted the player), `ring_bearer` (card ID, if any).

Game additionally: `monarch` / `initiative` = seat (−1 = nobody), `day` = true (day) / false
(night), absent as long as it is neither day nor night (CR 726).

The `state` query reads Forge's data from a thread other than the game thread; if Forge changes
something at the same time (`ConcurrentModificationException`), the bridge rereads up to ten times.

`combat` (current combat, otherwise empty lists): `attackers` = list of `{card, target}` (`target`
as `Target`), `blocks` = list of `[blocker, attacker]`.

`stack` (CR 405, like `GameState.stack` in forge-rules-rs): list of entries, **bottom first, top
last**. Per entry `id` (Forge's stack ID), `kind` (`Spell` / `Ability` / `Trigger`), `source`
(card ID of the source), `controller` (seat), `text` (Forge's stack description), `targets`
(targets per ability of the sub-ability chain: list of lists of `Target`) and for spells `card`
(card data of the card in the stack zone, as in the player zones). A spell as a target (e.g. a
counterspell) appears as `{"card": id}` of its card; an ability as a target as the card of its
source (the base model's `Target` has no stack entries).
On top are cards that are **being cast right now** (CR 601.2a: already in the stack zone, targets
and costs still open): `kind` `"Casting"`, `id` = minus the card ID, `text` empty, `targets`
empty, `card` as for spells. That way the card stays visible during target selection (start
point of the target arrow).

```json
"stack": [
  { "id": 28, "kind": "Spell", "source": 52, "controller": 0, "targets": [[]],
    "text": "Flametongue Kavu - Creature 4 / 2", "card": { "id": 52, "name": "Flametongue Kavu", … } },
  { "id": 31, "kind": "Spell", "source": 92, "controller": 1, "targets": [[{"card": 52}]],
    "text": "Mana Leak (92) - Counter Flametongue Kavu (52) unless its controller pays {3}.", "card": { … } }
]
```

Seats (`players`, `seat`, `controller`, `player` in targets and events) are the position in
Forge's `getRegisteredPlayers()` and stay fixed, even when a player leaves the game.

Phases (`phase`): Forge's `PhaseType` names, e.g. `UNTAP`, `UPKEEP`, `DRAW`, `MAIN1`,
`COMBAT_BEGIN`, `COMBAT_DECLARE_ATTACKERS`, `COMBAT_DECLARE_BLOCKERS`, `COMBAT_DAMAGE`,
`COMBAT_END`, `MAIN2`, `END_OF_TURN`, `CLEANUP`.

## Godot side: `ForgeBridge` (`rust/src/forge/bridge.rs`)

Example (GDScript), player 0 is remote-controlled, player 1 is the Forge AI:

```gdscript
@onready var forge: ForgeBridge = $ForgeBridge

func _ready():
    forge.bridge_ready.connect(func(_p):
        forge.start_match(
            PackedStringArray(["res://../forge-bridge/testdecks/rot.dck",
                               "res://../forge-bridge/testdecks/gruen.dck"]),
            PackedInt32Array([0]), PackedStringArray(["Player", "Opponent"]), -1))
    forge.forge_event.connect(func(e): print(e.event, ": ", e.text))
    forge.decision_requested.connect(_on_request)
    forge.game_over.connect(func(r): print("Winner: ", r.winner))
    forge.start_bridge()

func _on_request(r: Dictionary):
    match r.decision.type:
        "Mulligan":
            forge.answer(r.id, {"type": "Keep"})
        "Priority":
            # offer the choice in the UI here; the first action is always {"type": "Pass"}
            forge.answer(r.id, {"type": "Action", "action": r.decision.actions[0]})
        _:
            forge.answer(r.id, {"type": "Cancel"})   # the engine decides by itself
```

All signals and methods are listed in the [API reference](API-Referenz.md) under `ForgeBridge`.

## Forge on the 3D table

`godot/table.tscn` (main scene) plays a Forge match on the 3D table (cards, zones, drag and drop,
HUD). The `ForgeTable` node (`rust/src/forge/table.rs`) translates:

| Forge | 3D table |
|---|---|
| `state` (game start, every request, batched after events) | one `Card3D` per card `id` in hand / resource row (lands) / battlefield / discard pile, opponent's hand face down, tapped; `library` = face-down pile in the deck zone, new hand cards are drawn from there |
| `Priority` | cards with an action glow; dragging a hand card onto the table → `PlayLand`/`Cast`; if the card has several actions (without mana abilities) a 3D stage "Choose one" with one card copy per action; clicking a card outside the hand → its `Activate` or `ActivateMana` action (several: "Choose one" stage, mana abilities while paying: menu; pure mana sources do not glow); main button / space bar → `Action::Pass` |
| `ChooseTargets` | exactly one candidate with `min` = `max` = 1: `Targets([…])` immediately (no real choice). Otherwise no window: a golden arrow hangs from the source (card on the stack or the ability's card) to the mouse and snaps onto valid targets (cards glow, medallions pulse golden); clicking, or dragging and releasing the arrow, chooses (`ForgeTable.pick_candidate`), again = deselect. Once `max` targets are chosen, `Targets` goes to Forge immediately; otherwise the phase button "Confirm targets (n) »" (from `min`) or "Cancel" (`Cancel`). What the card asks for (`description`) is shown in gold above the timeline |
| `ChooseCards` with cards that all lie face up on the table (battlefield, land row, command zone, stack) | table mode: cards glow, a click chooses (gold), window at the right edge with card names and "Done" → `Cards` |
| `Mulligan` / `MulliganBottom` | 3D stage (`Showcase`): hand large in front of the camera, `Button3D` "Mulligan"/"Keep" → `Mulligan` / `Keep`; for `MulliganBottom` click cards, "Done" → `Cards` |
| `ChooseCards`, all options in the player's own hand (discard, `DiscardToHandSize`, costs …) | hand stage like `MulliganBottom` (`ForgeTable.show_hand_pick`): click cards, "Done" (active once the number is valid) → `Cards`, "Cancel" → `Cancel` (only if `cancelable`) |
| `ChooseModes` | 3D stage: one card copy per mode with only that mode as rules text → `Modes` |
| `Confirm`, `OptionalTrigger`, `PayUnlessCost`, `PayLifeForPhyrexian`, `ChooseOption` with 2 options | 3D stage: source card large, two buttons stacked (orange = first answer, blue = second) → `Yes`/`No` or `Number` |
| `ChooseOptions` with `purpose` `OptionalCost` | 3D stage: card large, orange "Kicker – {R}" → `Modes([0])`, grey "Normal" → `Modes([])`, blue "Cancel" (or Escape) → `Cancel` |
| `Scry` | 3D stage: box with "BOTTOM"/"GRAVEYARD" and "TOP", drag cards → `Arrange { top, bottom }` (rightmost card on top) |
| `ChooseCards` (not hand, not table) | 3D stage with a card fan and a slider; click → `Cards([id])` for one card, otherwise selection + "Done"; "Decline" → `Cards([])` |
| `OrderTriggers` | 3D stage "Set order": one card per entry (copy of the source from `sources`, text = description), a click assigns 1, 2, …, clicking again takes it back; "Done" → `Order` |
| `AssignCombatDamage` / `DivideAmount` | `DivideAmount`: +/- buttons and the amount at each target on the table; `AssignCombatDamage`: stage with the blockers (lethal damage prefilled) → `Damage` |
| `PayMana` | `ForgeTable` always answers `Pay(AutoPay)` immediately (no window; Forge also uses mana tapped in advance into the pool). Lands tapped in advance with unspent mana: click with priority → `Action(UndoMana)`; mana pool display from `mana_pool` |
| `DeclareAttackers` / `DeclareBlockers` | candidates glow blue, chosen ones orange (a blocker's frame fades to orange and flashes, a flickering trail rises above its edge facing the attacker); once declared (own blocks or `combat.blocks`) attacker and blocker turn yellow (frame, attack spike, trail, arrow); blocked attackers and blockers (plan and `combat.blocks`) each lie in their own pile, a blocked attacker directly opposite its blocker, the other attackers beside it; attackers move forward (attack lunge), attack arrows (orange) only with more than one attack target; dragging an arrow (creature → player/planeswalker or blocker → attacker; `TableController.aim_released`) or clicking plans an attack or block (red / orange arrows, markers; after clicking a blocker its arrow follows the mouse until an attacker is clicked), the phase button at the bottom right sends the plan (`ForgeTable.pass_priority`: "2 Attackers »" → `Attackers`, "No Blocks »" / "1 Blocker »" → `Blocks`; without chosen attackers it reads "All Attack" and only chooses every candidate), the button above it (`ForgeTable.press_secondary`) answers "No Attacks" with `Attackers([])` or only clears the plan ("Cancel Attacks", "Cancel Blocks"); "End turn" (`ForgeTable.end_turn`) answers `Pass` or `Attackers([])` by itself until the end of the turn; `combat` from the state shows the AI's combat as arrows |
| all other decisions | HUD choice window (`decision_view.rs`): buttons, multiple choice with "Done", number field; source card highlighted in gold |
| `exile`, `attached_to`, `counters`, `token` | exile pile per side (`PlayerSeat.exile`); attached cards offset under their host in its zone; counters and "token" as plaques on the card; player counters beside the medallion |
| `command`, `face`, `face_down`, `monarch`, `initiative`, `day`, `ring_level` | no command zone on the table: commanders (`commander`) appear only as a face-up copy at the right end of their owner's hand fan (the opponent's too) with `cast_cost`, glowing only while castable (`Priority.elsewhere`); emblems, dungeons and other effect cards at the right end of the battlefield's back row; face changes as a flip animation, face-down cards with a card-back pattern; plaques "Commander", "Ring-bearer", "Room: …", "↺ other face"; monarch, initiative, city's blessing, Ring level as emblems beside the medallion, day/night beside the turn number |
| `log` | currently not displayed (planned: an expandable 3D scroll) |
| `stack` | `Stack` zone on the right as a pile of cards (newest entry in front, older ones offset behind): spells (including `Casting`) as their card, abilities as the card of their source; arrows to the targets in the colour of the source card, as an arc to entries below; no separate list in the HUD, the 3D pile shows all entries |
| `phase`, `life`, `turn`, `game_over` | timeline, medallions, turn number, game-over message in the `TurnHud` |

## Status and test coverage

- Tested (headless): Godot node → bridge → Forge 2.0.15, a match in `table.tscn` until the end of
  the game; cards move correctly between hand, resources, battlefield and graveyard.
- Tested with the project copy `forge-bridge/forge/` (AI against AI until the end of the game).
- There is no rules implementation of our own: Forge is the only rules engine.
- General choices tested with `forge-bridge/testdecks/auswahl.dck` (starting player, creature
  type, card name, mode, colour) and with synthetic requests (multiple choice, yes/no, number,
  cancel).
- Targets, X value, kicker, modes and equip tested with `testdecks/spieler.dck` against
  `testdecks/gegner.dck` (default decks of `ForgeTable`).
- Protocol v3 tested: starting player (`ChooseOption`), `Mulligan`, `NameCard` (34,457 names,
  search), kicker (`ChooseOptions`), colour, `ChooseTargets` for equip, `Priority` with `Cast` and
  `Activate`.
- Targets by click (card, medallion, multiple targets, arrows) and activation by click (including
  menu) tested; attacking and blocking by click tested (attacking with 1–3 creatures, "no
  attack", blocking 1–3 attackers, the AI's combat arrows from `combat`).
- Paying mana tested: land clicks until paid (e.g. {2}{R} → {2} → {1}), "Auto",
  kicker/modes/X/targets via Forge's human flow.
- Card fan tested with `testdecks/kartenwahl.dck`: `MulliganBottom` (7 cards, 1 to the bottom of
  the library), `Scry` (Magma Jet, one card to the bottom), `ChooseCards` with `Search`
  (Wayfarer's Bauble, 20 cards from the library).
- Costs, dividing, Phyrexian mana and trigger targets tested with `testdecks/kosten.dck` against
  `testdecks/gegner_counter.dck`: Gut Shot (2 life), Forked Bolt (1+1), Cathartic Reunion
  (discard 2 cards as a cost), Flametongue Kavu (trigger target); combat damage assignment with a
  synthetic request (prefill 2/3/1 with trample, sum check).
- Stack tested with `testdecks/kosten.dck` against `testdecks/gegner_counter.dck`: spells of both
  sides in the stack zone, Mana Leak on Flametongue Kavu (2 entries, target = spell on the stack);
  triggered ability (Flametongue Kavu's trigger with a target) as the card of its source, priority
  with a non-empty stack.
- Exile, counters, tokens and attached cards tested with a test deck (Dragon Fodder, Bonesplitter,
  Chandra, Torch of Defiance, Magma Spray) against `testdecks/gegner.dck` (Rancor): tokens,
  `LOYALTY`, exile of both sides, Rancor on the opponent's creature, equipment on its host. Player
  counters only checked by unit test.
- Card image from card data tested (headless and window close-up): rules text, symbols (↷, Ⓡ, ①),
  type lines, planeswalker loyalty (printed in hand, current in play), font fitting for long text
  (Chandra, Bonesplitter).
- Command zone, faces and designations tested with a test deck (Brutal Cathar, Soul Summons,
  Seasoned Dungeoneer, Palace Jailer, Call of the Ring) against `gegner.dck`: The Ring, The
  Monarch, The Initiative, Undercity with room, Palace Jailer's effect in the command zone;
  manifest (face-down 2/2); day/night and transforming Brutal Cathar ↔ Moonrage Brute.
- Effect choices tested with a test deck (Sawhorn Nemesis, Goblin Bombardment, Raging Goblin):
  "Choose a player" and "any target" by clicking the medallion, sacrificing as a cost by clicking
  the creature (table mode). Blocker ordering was not triggered (the AI never double-blocked).
- Delve, convoke, improvise tested with a test deck (Stoke the Flames, Treasure Cruise, Reverse
  Engineer, Raging Goblin, Ornithopter) against `gegner.dck`: creatures or artifacts in table mode,
  graveyard cards in the card fan; only asked during real casting (not during playability checks).
- Exile as a cost tested with Grim Lavamancer (two cards from the own graveyard, card fan).
- Targets on the stack are now the spells/abilities themselves (previously the card, which made
  copying fail); in `ChooseTargets` they appear as the card of their source. Tested with
  Reverberate on Shock: copy, "Choose new targets?", new target selection. Exert (Ahn-Crop
  Crasher) and enlist (Yavimaya Steelcrusher) tested in table mode.
- Cascade (Bloodbraid Elf → "Do you want to play Shock?" → target), Fact or Fiction (pile choice)
  and Leyline of Punishment from the opening hand tested.
- Crew tested with Smuggler's Copter (Crew 1) and Raging Goblin (table mode).
- `chooseManaFromPool`, splice, group choice and target change are remote-controlled but not
  triggered deliberately (only regression test with the default test decks).
- Reveal tested: the Forge AI reveals the searched card from the library (card fan without
  selection).
- Pool mana by click tested with Rite of Flame (RR into the pool) and Lightning Bolt
  (`UsePool R`).
- Modes with repetition (`can_repeat`) tested with Fiery Confluence: the same mode three times.
- Dividing a prevention shield and swapping dice rolls are remote-controlled but not triggered
  deliberately.
- Colour combination tested with Manamorphose (two colour choices, then pool mana by click).
- Game log tested: 440 entries in one match, chronological (mulligan, turns, lands, spells with
  targets, resolving, zone changes).
- Forge's own question texts are in English (Forge language setting); the bridge's own texts are
  German.
