<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p rune-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/rune-engine/data/catalog/ + crates/rune-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

RUNE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (36)

Every functional definition in `crates/rune-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0018 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
| `bramble_hatchling` | Bramble Hatchling | functional definition |
| `bramblefang_spider` | Bramblefang Spider | functional definition |
| `cinder_shock` | Cinder Shock | functional definition |
| `cleric_of_the_sunwell` | Cleric of the Sunwell | functional definition |
| `copper_lodestone` | Copper Lodestone | functional definition |
| `cryptvine_lurker` | Cryptvine Lurker | functional definition |
| `dawnblade_duelist` | Dawnblade Duelist | functional definition |
| `emberfang_jackal` | Emberfang Jackal | functional definition |
| `emberrush_raider` | Emberrush Raider | functional definition |
| `forest` | Forest | functional definition |
| `gorehorn_ravager` | Gorehorn Ravager | functional definition |
| `hurried_study` | Hurried Study | functional definition |
| `ironbark_aegis` | Ironbark Aegis | functional definition |
| `ironwatch_sentinel` | Ironwatch Sentinel | functional definition |
| `island` | Island | functional definition |
| `mountain` | Mountain | functional definition |
| `nettle_adder` | Nettle Adder | functional definition |
| `plains` | Plains | functional definition |
| `quickfire_bolt` | Quickfire Bolt | functional definition |
| `riverbank_otter` | Riverbank Otter | functional definition |
| `runic_negation` | Runic Negation | functional definition |
| `skywhisker_drake` | Skywhisker Drake | functional definition |
| `soothing_balm` | Soothing Balm | functional definition |
| `stonehide_basilisk` | Stonehide Basilisk | functional definition |
| `sunder_ray` | Sunder Ray | functional definition |
| `swamp` | Swamp | functional definition |
| `thornback_boar` | Thornback Boar | functional definition |
| `thornweft_sprite` | Thornweft Sprite | functional definition |
| `titanroot_surge` | Titanroot Surge | functional definition |
| `verdant_blessing` | Verdant Blessing | functional definition |
| `verdant_sanctuary` | Verdant Sanctuary | functional definition |
| `verdant_scout` | Verdant Scout | functional definition |
| `vexing_ordeal` | Vexing Ordeal | functional definition |
| `viridian_baneclaw` | Viridian Baneclaw | functional definition |
| `witherbrand_curse` | Witherbrand Curse | functional definition |
| `withering_touch` | Withering Touch | functional definition |

## Excluded (6)

Cards and mechanics considered and deliberately left out of scope, each with the blocker that keeps it out. Names and blockers only — no rules text. Curated by hand in `crates/rune-engine/data/exclusions.json`.

| Excluded | Blocker |
| --- | --- |
| Auras that enchant a player or land, or move between hosts | only P/T-granting enchant-creature Auras are modeled |
| Continuous keyword-granting effects | only printed keywords are modeled; effects that grant keywords are not |
| Double strike | a creature deals combat damage in exactly one step; dealing in both the first-strike and regular steps is not modeled (issue #346) |
| Multi-face cards (transform, modal double-faced) | the card model has a single face |
| Planeswalkers | no loyalty counter system or loyalty abilities |
| Replacement effects | no replacement-effect layer in the rules engine |
