<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p sage-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/sage-engine/data/catalog/ + crates/sage-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

SAGE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (94)

Every functional definition in `crates/sage-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0008 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
| `aegis_of_the_heavens` | Aegis of the Heavens | functional definition |
| `aggressive_mammoth` | Aggressive Mammoth | functional definition |
| `air_elemental` | Air Elemental | functional definition |
| `ancient_brontodon` | Ancient Brontodon | functional definition |
| `angel_of_the_dawn` | Angel of the Dawn | functional definition |
| `boggart_brute` | Boggart Brute | functional definition |
| `bogstomper` | Bogstomper | functional definition |
| `bone_to_ash` | Bone to Ash | functional definition |
| `cancel` | Cancel | functional definition |
| `centaur_courser` | Centaur Courser | functional definition |
| `child_of_night` | Child of Night | functional definition |
| `cinder_barrens` | Cinder Barrens | functional definition |
| `colossal_dreadmaw` | Colossal Dreadmaw | functional definition |
| `crash_through` | Crash Through | functional definition |
| `daggerback_basilisk` | Daggerback Basilisk | functional definition |
| `daybreak_chaplain` | Daybreak Chaplain | functional definition |
| `diregraf_ghoul` | Diregraf Ghoul | functional definition |
| `divination` | Divination | functional definition |
| `druid_of_the_cowl` | Druid of the Cowl | functional definition |
| `dwarven_priest` | Dwarven Priest | functional definition |
| `electrify` | Electrify | functional definition |
| `essence_scatter` | Essence Scatter | functional definition |
| `field_creeper` | Field Creeper | functional definition |
| `fire_elemental` | Fire Elemental | functional definition |
| `flight` | Flight | functional definition |
| `forest` | Forest | functional definition |
| `forsaken_sanctuary` | Forsaken Sanctuary | functional definition |
| `foul_orchard` | Foul Orchard | functional definition |
| `giant_spider` | Giant Spider | functional definition |
| `gigantosaurus` | Gigantosaurus | functional definition |
| `goblin_motivator` | Goblin Motivator | functional definition |
| `greenwood_sentinel` | Greenwood Sentinel | functional definition |
| `havoc_devils` | Havoc Devils | functional definition |
| `herald_of_faith` | Herald of Faith | functional definition |
| `highland_game` | Highland Game | functional definition |
| `highland_lake` | Highland Lake | functional definition |
| `hostile_minotaur` | Hostile Minotaur | functional definition |
| `inspired_charge` | Inspired Charge | functional definition |
| `invoke_the_divine` | Invoke the Divine | functional definition |
| `island` | Island | functional definition |
| `jedit_ojanen` | Jedit Ojanen | functional definition |
| `jump` | Jump | functional definition |
| `knight_of_the_tusk` | Knight of the Tusk | functional definition |
| `knight_s_pledge` | Knight's Pledge | functional definition |
| `lava_axe` | Lava Axe | functional definition |
| `lich_s_caress` | Lich's Caress | functional definition |
| `lightning_strike` | Lightning Strike | functional definition |
| `llanowar_elves` | Llanowar Elves | functional definition |
| `loxodon_line_breaker` | Loxodon Line Breaker | functional definition |
| `meandering_river` | Meandering River | functional definition |
| `mighty_leap` | Mighty Leap | functional definition |
| `millstone` | Millstone | functional definition |
| `mountain` | Mountain | functional definition |
| `murder` | Murder | functional definition |
| `naturalize` | Naturalize | functional definition |
| `oakenform` | Oakenform | functional definition |
| `onakke_ogre` | Onakke Ogre | functional definition |
| `oreskos_swiftclaw` | Oreskos Swiftclaw | functional definition |
| `pelakka_wurm` | Pelakka Wurm | functional definition |
| `plains` | Plains | functional definition |
| `plummet` | Plummet | functional definition |
| `prodigious_growth` | Prodigious Growth | functional definition |
| `revitalize` | Revitalize | functional definition |
| `rhox_oracle` | Rhox Oracle | functional definition |
| `rustwing_falcon` | Rustwing Falcon | functional definition |
| `sanctuary_cat` | Sanctuary Cat | functional definition |
| `serra_angel` | Serra Angel | functional definition |
| `shock` | Shock | functional definition |
| `skeleton_archer` | Skeleton Archer | functional definition |
| `skyscanner` | Skyscanner | functional definition |
| `smelt` | Smelt | functional definition |
| `snapping_drake` | Snapping Drake | functional definition |
| `sovereign_s_bite` | Sovereign's Bite | functional definition |
| `stone_quarry` | Stone Quarry | functional definition |
| `strangling_spores` | Strangling Spores | functional definition |
| `submerged_boneyard` | Submerged Boneyard | functional definition |
| `sure_strike` | Sure Strike | functional definition |
| `swamp` | Swamp | functional definition |
| `take_vengeance` | Take Vengeance | functional definition |
| `thornhide_wolves` | Thornhide Wolves | functional definition |
| `timber_gorge` | Timber Gorge | functional definition |
| `titanic_growth` | Titanic Growth | functional definition |
| `tolarian_scholar` | Tolarian Scholar | functional definition |
| `trained_caracal` | Trained Caracal | functional definition |
| `tranquil_expanse` | Tranquil Expanse | functional definition |
| `trusty_packbeast` | Trusty Packbeast | functional definition |
| `unsummon` | Unsummon | functional definition |
| `vampire_neonate` | Vampire Neonate | functional definition |
| `viashino_pyromancer` | Viashino Pyromancer | functional definition |
| `vigilant_baloth` | Vigilant Baloth | functional definition |
| `volcanic_dragon` | Volcanic Dragon | functional definition |
| `walking_corpse` | Walking Corpse | functional definition |
| `wall_of_mist` | Wall of Mist | functional definition |
| `woodland_stream` | Woodland Stream | functional definition |

## Excluded (30)

Cards and mechanics considered and deliberately left out of scope, each with the blocker that keeps it out. Names and blockers only — no rules text. Curated by hand in `crates/sage-engine/data/exclusions.json`.

| Excluded | Blocker |
| --- | --- |
| Abilities that trigger on a phase or step | no upkeep, draw-step, end-step, or beginning-of-combat trigger condition |
| Abilities that trigger on another object | trigger conditions observe only their own source |
| Abilities that trigger on casting, drawing, or gaining life | trigger conditions observe only zone changes and attack declaration |
| Attack and block requirements and restrictions | only defender restricts declaration; must-attack and cannot-block are unmodeled |
| Auras that enchant a player or land, or move between hosts | only P/T- and keyword-granting enchant-creature Auras are modeled |
| Conditional effects and intervening-if clauses | no condition attached to an effect or a trigger |
| Cost reduction and cost increase effects | no cost-modification layer |
| Costs paid by sacrificing or discarding | activation costs model only tapping and mana |
| Damage prevention | no prevention shield or damage-replacement layer |
| Effects that ask a player to name a colour, type, or card | no player choice recorded on a permanent or spell |
| Effects that count permanents, cards, or other game values | effect amounts are fixed numbers, never derived from game state |
| Effects that make a player discard | no prompt for a player to choose cards to discard |
| Effects that return cards from a graveyard | no graveyard-to-hand or graveyard-to-battlefield effect |
| Effects that search, reveal, or reorder a library | no library search, reveal, or scry choice |
| Effects that untap, or that stop a permanent untapping | no untap effect and no skipped-untap flag |
| Equipment | no equip action or attachment outside the Aura model |
| Evasion other than flying, reach, and menace | no unblockable, protection, or colour- and count-restricted blocking |
| Fight, and other effects taking two targets | each effect fills exactly one target slot |
| Gaining control of a permanent | no control-change layer |
| Keyword removal and loses-all-abilities effects | the ability-adding layer only adds abilities |
| Kicker and other optional additional costs | no optional cost declared on announcement |
| Mana of any colour, and mana filtering | mana production names one fixed colour or colourless |
| Maximum hand size modification | the cleanup discard uses a fixed hand size |
| Modal spells that choose one | no mode choice on announcement |
| Multi-face cards (transform, modal double-faced) | the card model has a single face |
| Planeswalkers | no loyalty counter system or loyalty abilities |
| Replacement effects | no replacement-effect layer in the rules engine |
| Spells with X in their cost | mana costs are fixed strings with no X announcement |
| Token creation | no token object model; every permanent needs a catalog card |
| Triggered abilities that choose targets | a trigger reaches the stack with no targets and nothing prompts its controller to choose |
