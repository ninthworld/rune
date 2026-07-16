# Card compatibility report

<!--
GENERATED FILE — do not edit by hand.
Regenerate with:  cargo test -p rune-engine regenerate_compatibility_report -- --ignored
Sources:          crates/rune-engine/data/catalog/  +  crates/rune-engine/data/exclusions.json
-->

RUNE claims support only for the verified slice of cards listed here — never a full set. Every supported card is a hand-authored functional definition (ADR 0018); the excluded list names mechanics considered and deliberately left out of scope, each with the blocker keeping it there. No Oracle text, flavor text, or branding appears in this report or its sources.

## Supported cards (36)

| functional_id | name | implementation |
| --- | --- | --- |
| bramble_hatchling | Bramble Hatchling | functional |
| bramblefang_spider | Bramblefang Spider | functional |
| cinder_shock | Cinder Shock | functional |
| cleric_of_the_sunwell | Cleric of the Sunwell | functional |
| copper_lodestone | Copper Lodestone | functional |
| cryptvine_lurker | Cryptvine Lurker | functional |
| dawnblade_duelist | Dawnblade Duelist | functional |
| emberfang_jackal | Emberfang Jackal | functional |
| emberrush_raider | Emberrush Raider | functional |
| forest | Forest | functional |
| gorehorn_ravager | Gorehorn Ravager | functional |
| hurried_study | Hurried Study | functional |
| ironbark_aegis | Ironbark Aegis | functional |
| ironwatch_sentinel | Ironwatch Sentinel | functional |
| island | Island | functional |
| mountain | Mountain | functional |
| nettle_adder | Nettle Adder | functional |
| plains | Plains | functional |
| quickfire_bolt | Quickfire Bolt | functional |
| riverbank_otter | Riverbank Otter | functional |
| runic_negation | Runic Negation | functional |
| skywhisker_drake | Skywhisker Drake | functional |
| soothing_balm | Soothing Balm | functional |
| stonehide_basilisk | Stonehide Basilisk | functional |
| sunder_ray | Sunder Ray | functional |
| swamp | Swamp | functional |
| thornback_boar | Thornback Boar | functional |
| thornweft_sprite | Thornweft Sprite | functional |
| titanroot_surge | Titanroot Surge | functional |
| verdant_blessing | Verdant Blessing | functional |
| verdant_sanctuary | Verdant Sanctuary | functional |
| verdant_scout | Verdant Scout | functional |
| vexing_ordeal | Vexing Ordeal | functional |
| viridian_baneclaw | Viridian Baneclaw | functional |
| witherbrand_curse | Witherbrand Curse | functional |
| withering_touch | Withering Touch | functional |

## Excluded mechanics (8)

| mechanic | blocker |
| --- | --- |
| Battles | no battle-type or siege/defense model |
| Double-faced cards | no transform or multi-face card model |
| Fetch and search effects | no library search or shuffle-from-play |
| Mana-producing creatures | creatures carry no activated mana abilities yet |
| Modal spells | no choose-one-or-more spell modes |
| Planeswalkers | no loyalty or planeswalker-type system |
| Regeneration | no destruction-replacement (regeneration shield) effect |
| Variable {X} costs | no variable-cost payment during casting |
