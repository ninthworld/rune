# Bundled presentation assets

Every asset below is original work made for RUNE, compressed to shipping WebP
files and distributed under the repository's MIT license. The machine-readable
authority is `ledger.json`, which records the provenance class and the
generating tool per asset.

The illustrated sets — environments, portraits, card backs, card art — are
ADR 0031 provenance **class 2** (original AI-generated work). Two tools
produced them: environment, portrait, and card-back art with OpenAI's built-in
image generation, and card art locally with Chroma1-HD via ComfyUI — an
Apache-2.0 base model, chosen because its output carries no downstream licence
restriction that would conflict with redistributing these files under this
repository's MIT terms.

The **card-frame plates** are provenance **class 1** (original created work),
because the tool for them is arithmetic rather than a model: they are
synthesised by `../../scripts/generateFramePlates.js`, a committed,
deterministic generator, and re-running `npm run frames` reproduces the same
bytes. They are also the only set that is not a picture — each plate is an
alpha **light map** (bevel, shadow, grain, and the structural gold hairline)
composed over the token colours, which is why one set serves both environment
themes and all eight colour identities. See `docs/design/card-representation.md`
§3.12.

No prompt referenced an existing game, publisher, protected property, or named
artist as a style target. Card art is composed from RUNE's own functional card
data — type, subtype, colour, and keywords — rendered in one house style
(painterly digital illustration, cinematic light, landscape composition) so the
set reads as a single commission.

| Asset | Category | Prompt essence |
| --- | --- | --- |
| Runic Vale — Far Surround | environment | Soft teal water, distant foliage, rock banks, upper-left glow |
| Runic Vale — Arena Floor (Half) | environment | Half-resolution pale plaza, radial paving, abstract medallion |
| Runic Vale — Arena Floor | environment | Isolated low-contrast pale plaza registered to the 21:9 camera |
| Runic Vale — Arena Edge | environment | Isolated mossy perimeter and top/bottom depth lips |
| Runic Vale — Prop Atlas | environment props | Six isolated moss, lantern, flower, stone, and crystal clusters |
| Verdant Canals — Key-Art Study | environment study | Sand plaza, cyan canals, reeds, foliage, restrained brass |
| Sunlit Observatory — Key-Art Study | environment study | Ochre plaza, pale gold light, pools, astronomical instruments |
| Moonlit Ruins — Key-Art Study | environment study | Slate plaza, ruined arcades, moonlight, cyan streams |
| Verdant Canals — Far Surround | environment | Cyan canals, wet stone, distant foliage and reeds |
| Verdant Canals — Arena Floor (Half) | environment | Half-resolution derivative of the pale canal plaza |
| Verdant Canals — Arena Floor | environment | Isolated pale canal plaza, concentric paving, medallion |
| Verdant Canals — Arena Edge | environment | Mossy canal rim and the shared lip bands |
| Verdant Canals — Prop Atlas | environment props | Six canal-stone, water, reed, lantern, shrub, and lily props |
| Sunlit Observatory — Far Surround | environment | Warm terraces, pools, gardens, and golden haze |
| Sunlit Observatory — Arena Floor (Half) | environment | Half-resolution derivative of the ochre plaza |
| Sunlit Observatory — Arena Floor | environment | Isolated ochre plaza, concentric paving, astronomical medallion |
| Sunlit Observatory — Arena Edge | environment | Warm carved-stone rim and the shared lip bands |
| Sunlit Observatory — Prop Atlas | environment props | Six brass armillary, telescope, lantern, dial, and sundial props |
| Moonlit Ruins — Far Surround | environment | Blue-violet valley, cyan streams, distant ruins, moonlight |
| Moonlit Ruins — Arena Floor (Half) | environment | Half-resolution derivative of the lifted-value ruin plaza |
| Moonlit Ruins — Arena Floor | environment | Isolated lifted-value plaza, paving rings, medallion |
| Moonlit Ruins — Arena Edge | environment | Broken luminous ruin rim and the shared lip bands |
| Moonlit Ruins — Prop Atlas | environment props | Six arch, obelisk, lantern, column, crystal, and rune-stone props |
| Local Hood | portrait | Faceless forward-facing avatar in a deep blue hood |
| Silver Scholar | portrait | Older scholar with close silver hair and teal scarf |
| Ember Topknot | portrait | Young man with high topknot and ochre collar |
| Frost Braid | portrait | Freckled woman with white bob and side braid |
| Stone Moustache | portrait | Shaved-head man with long gray moustache |
| Forked Beard | portrait | Elderly man with long forked white beard |
| Twin Braids | portrait | Young woman with symmetric braided side buns |
| Copper Curls | portrait | Adult with compact copper curls and teal cape |
| Indigo Wrap | portrait | Older woman with tall gold-and-indigo wrapped headdress |
| Rune Spiral | card back | Navy/slate, centered four-arm gold spiral, mirrored edge rule |
| Verdant Knot | card back | Forest teal, centered six-petal brass knot, symmetric edge rule |
| Onakke Ogre | card art | Hulking ogre with a crude weapon on a lava-seamed ridge |
| Viashino Pyromancer | card art | Lizard-folk mage channelling arcane fire on volcanic rock |
| Fire Elemental | card art | Towering elemental of living flame above a caldera |
| Volcanic Dragon | card art | Immense winged dragon aloft over a glowing ridge |
| Shock | card art | Forking bolt bursting against shattering rock at night |
| Lightning Strike | card art | Lance of white-hot lightning splitting stone in darkness |
| Electrify | card art | Crimson energy bolt shattering rock in a spray of sparks |
| Mountain | card art | Jagged volcanic peak with glowing lava seams |
| Llanowar Elves | card art | Slender pointed-eared elf in dense old forest |
| Druid of the Cowl | card art | Vine-entwined elven druid among ferns |
| Giant Spider | card art | Enormous long-limbed spider in green undergrowth |
| Rustwing Falcon | card art | Bird of prey banking over sunlit highland plains |
| Serra Angel | card art | Winged armoured celestial warrior above open plains |
| Colossal Dreadmaw | card art | Armoured reptilian giant crushing forest floor |
| Gigantosaurus | card art | Colossal dinosaur dwarfing the canopy around it |
| Trusty Packbeast | card art | Heavy horned draft ox on golden highland grass |
| Titanic Growth | card art | Green empowering aura spiralling around a lone figure |
| Revitalize | card art | Column of restorative gold-white light with drifting motes |
| Forest | card art | Old-growth forest, mossy trunks, light through canopy |
| Plains | card art | Sunlit grassland rolling toward distant hills |
| Tranquil Expanse | card art | Untouched wilderness under a wide dramatic sky |
| Snapping Drake | card art | Lean winged drake over mist-wreathed coastal cliffs |
| Air Elemental | card art | Towering elemental of wind and cloud above deep water |
| Skyscanner | card art | Small clockwork flier of brass and canvas over bare rock |
| Tolarian Scholar | card art | Robed wizard channelling arcane light by the sea |
| Divination | card art | Spiral of glowing sigils and parchment above an open book |
| Cancel | card art | Hexagonal barrier of force flaring as a spell shatters |
| Island | card art | Lone rocky island ringed by calm reflective sea |
| Jedit Ojanen | card art | Powerful great-cat warrior in deep green forest |
| Layered edge and inner border | card frame | Outer contour, lit slate bevel, thicker bottom paper edge, engraved gold hairline |
| Art window surround | card frame | Recessed lip: shadowed on the light side, catching light on the far side |
| Name/cost header field | card frame | Raised printed field — lit top rim, shaded lower rim, paper grain |
| Lower information strip | card frame | Recessed printed strip the type line and rules text sit in |
| Status band surface | card frame | Slate channel cut a shade deeper, so its plates read as objects lying in it |
| P/T plate | card frame | Small strongly bevelled tile with a tighter radius — a distinct object |
| Colour-identity material | card frame | Seamless woven tile the identity surfaces are tinted through |
