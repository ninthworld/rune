<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p rune-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/rune-engine/data/catalog/ + crates/rune-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

RUNE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (61)

Every functional definition in `crates/rune-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0018 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
| `aegis_of_the_heavens` | Aegis of the Heavens | functional definition |
| `air_elemental` | Air Elemental | functional definition |
| `bogstomper` | Bogstomper | functional definition |
| `cancel` | Cancel | functional definition |
| `centaur_courser` | Centaur Courser | functional definition |
| `child_of_night` | Child of Night | functional definition |
| `colossal_dreadmaw` | Colossal Dreadmaw | functional definition |
| `daybreak_chaplain` | Daybreak Chaplain | functional definition |
| `diregraf_ghoul` | Diregraf Ghoul | functional definition |
| `divination` | Divination | functional definition |
| `druid_of_the_cowl` | Druid of the Cowl | functional definition |
| `electrify` | Electrify | functional definition |
| `field_creeper` | Field Creeper | functional definition |
| `fire_elemental` | Fire Elemental | functional definition |
| `flight` | Flight | functional definition |
| `forest` | Forest | functional definition |
| `giant_spider` | Giant Spider | functional definition |
| `gigantosaurus` | Gigantosaurus | functional definition |
| `greenwood_sentinel` | Greenwood Sentinel | functional definition |
| `havoc_devils` | Havoc Devils | functional definition |
| `highland_game` | Highland Game | functional definition |
| `hostile_minotaur` | Hostile Minotaur | functional definition |
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
| `mighty_leap` | Mighty Leap | functional definition |
| `mountain` | Mountain | functional definition |
| `murder` | Murder | functional definition |
| `oakenform` | Oakenform | functional definition |
| `onakke_ogre` | Onakke Ogre | functional definition |
| `oreskos_swiftclaw` | Oreskos Swiftclaw | functional definition |
| `pelakka_wurm` | Pelakka Wurm | functional definition |
| `plains` | Plains | functional definition |
| `prodigious_growth` | Prodigious Growth | functional definition |
| `revitalize` | Revitalize | functional definition |
| `rhox_oracle` | Rhox Oracle | functional definition |
| `rustwing_falcon` | Rustwing Falcon | functional definition |
| `serra_angel` | Serra Angel | functional definition |
| `shock` | Shock | functional definition |
| `skeleton_archer` | Skeleton Archer | functional definition |
| `skyscanner` | Skyscanner | functional definition |
| `snapping_drake` | Snapping Drake | functional definition |
| `strangling_spores` | Strangling Spores | functional definition |
| `sure_strike` | Sure Strike | functional definition |
| `swamp` | Swamp | functional definition |
| `titanic_growth` | Titanic Growth | functional definition |
| `tolarian_scholar` | Tolarian Scholar | functional definition |
| `trained_caracal` | Trained Caracal | functional definition |
| `tranquil_expanse` | Tranquil Expanse | functional definition |
| `trusty_packbeast` | Trusty Packbeast | functional definition |
| `viashino_pyromancer` | Viashino Pyromancer | functional definition |
| `vigilant_baloth` | Vigilant Baloth | functional definition |
| `volcanic_dragon` | Volcanic Dragon | functional definition |
| `walking_corpse` | Walking Corpse | functional definition |

## Excluded (4)

Cards and mechanics considered and deliberately left out of scope, each with the blocker that keeps it out. Names and blockers only — no rules text. Curated by hand in `crates/rune-engine/data/exclusions.json`.

| Excluded | Blocker |
| --- | --- |
| Auras that enchant a player or land, or move between hosts | only P/T- and keyword-granting enchant-creature Auras are modeled |
| Multi-face cards (transform, modal double-faced) | the card model has a single face |
| Planeswalkers | no loyalty counter system or loyalty abilities |
| Replacement effects | no replacement-effect layer in the rules engine |
