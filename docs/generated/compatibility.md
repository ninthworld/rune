<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p rune-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/rune-engine/data/catalog/ + crates/rune-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

RUNE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (37)

Every functional definition in `crates/rune-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0018 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
| `air_elemental` | Air Elemental | functional definition |
| `cancel` | Cancel | functional definition |
| `child_of_night` | Child of Night | functional definition |
| `colossal_dreadmaw` | Colossal Dreadmaw | functional definition |
| `diregraf_ghoul` | Diregraf Ghoul | functional definition |
| `divination` | Divination | functional definition |
| `druid_of_the_cowl` | Druid of the Cowl | functional definition |
| `electrify` | Electrify | functional definition |
| `fire_elemental` | Fire Elemental | functional definition |
| `flight` | Flight | functional definition |
| `forest` | Forest | functional definition |
| `giant_spider` | Giant Spider | functional definition |
| `gigantosaurus` | Gigantosaurus | functional definition |
| `island` | Island | functional definition |
| `jedit_ojanen` | Jedit Ojanen | functional definition |
| `jump` | Jump | functional definition |
| `lightning_strike` | Lightning Strike | functional definition |
| `llanowar_elves` | Llanowar Elves | functional definition |
| `mountain` | Mountain | functional definition |
| `murder` | Murder | functional definition |
| `onakke_ogre` | Onakke Ogre | functional definition |
| `plains` | Plains | functional definition |
| `revitalize` | Revitalize | functional definition |
| `rustwing_falcon` | Rustwing Falcon | functional definition |
| `serra_angel` | Serra Angel | functional definition |
| `shock` | Shock | functional definition |
| `skyscanner` | Skyscanner | functional definition |
| `snapping_drake` | Snapping Drake | functional definition |
| `swamp` | Swamp | functional definition |
| `titanic_growth` | Titanic Growth | functional definition |
| `tolarian_scholar` | Tolarian Scholar | functional definition |
| `trained_caracal` | Trained Caracal | functional definition |
| `tranquil_expanse` | Tranquil Expanse | functional definition |
| `trusty_packbeast` | Trusty Packbeast | functional definition |
| `viashino_pyromancer` | Viashino Pyromancer | functional definition |
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
