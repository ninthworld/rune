<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p sage-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/sage-engine/data/catalog/ + crates/sage-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

SAGE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (240)

Every functional definition in `crates/sage-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0008 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
| `abnormal_endurance` | Abnormal Endurance | functional definition |
| `act_of_treason` | Act of Treason | functional definition |
| `aegis_of_the_heavens` | Aegis of the Heavens | functional definition |
| `aerial_engineer` | Aerial Engineer | functional definition |
| `aether_tunnel` | Aether Tunnel | functional definition |
| `aethershield_artificer` | Aethershield Artificer | functional definition |
| `aggressive_mammoth` | Aggressive Mammoth | functional definition |
| `air_elemental` | Air Elemental | functional definition |
| `ajani_adversary_of_tyrants` | Ajani, Adversary of Tyrants | functional definition |
| `ajani_s_influence` | Ajani's Influence | functional definition |
| `ajani_s_pridemate` | Ajani's Pridemate | functional definition |
| `ajani_s_welcome` | Ajani's Welcome | functional definition |
| `alpine_moon` | Alpine Moon | functional definition |
| `angel_of_the_dawn` | Angel of the Dawn | functional definition |
| `anticipate` | Anticipate | functional definition |
| `arcades_the_strategist` | Arcades, the Strategist | functional definition |
| `arcane_encyclopedia` | Arcane Encyclopedia | functional definition |
| `arisen_gorgon` | Arisen Gorgon | functional definition |
| `aven_wind_mage` | Aven Wind Mage | functional definition |
| `aviation_pioneer` | Aviation Pioneer | functional definition |
| `banefire` | Banefire | functional definition |
| `befuddle` | Befuddle | functional definition |
| `blanchwood_armor` | Blanchwood Armor | functional definition |
| `blood_divination` | Blood Divination | functional definition |
| `boggart_brute` | Boggart Brute | functional definition |
| `bogstomper` | Bogstomper | functional definition |
| `bone_to_ash` | Bone to Ash | functional definition |
| `bristling_boar` | Bristling Boar | functional definition |
| `cancel` | Cancel | functional definition |
| `catalyst_elemental` | Catalyst Elemental | functional definition |
| `cavalry_drillmaster` | Cavalry Drillmaster | functional definition |
| `centaur_courser` | Centaur Courser | functional definition |
| `child_of_night` | Child of Night | functional definition |
| `cinder_barrens` | Cinder Barrens | functional definition |
| `cleansing_nova` | Cleansing Nova | functional definition |
| `colossal_dreadmaw` | Colossal Dreadmaw | functional definition |
| `colossal_majesty` | Colossal Majesty | functional definition |
| `court_cleric` | Court Cleric | functional definition |
| `crash_through` | Crash Through | functional definition |
| `crucible_of_worlds` | Crucible of Worlds | functional definition |
| `daggerback_basilisk` | Daggerback Basilisk | functional definition |
| `daybreak_chaplain` | Daybreak Chaplain | functional definition |
| `death_baron` | Death Baron | functional definition |
| `declare_dominance` | Declare Dominance | functional definition |
| `demon_of_catastrophes` | Demon of Catastrophes | functional definition |
| `detection_tower` | Detection Tower | functional definition |
| `diamond_mare` | Diamond Mare | functional definition |
| `diregraf_ghoul` | Diregraf Ghoul | functional definition |
| `dismissive_pyromancer` | Dismissive Pyromancer | functional definition |
| `disperse` | Disperse | functional definition |
| `divination` | Divination | functional definition |
| `doomed_dissenter` | Doomed Dissenter | functional definition |
| `draconic_disciple` | Draconic Disciple | functional definition |
| `dragon_egg` | Dragon Egg | functional definition |
| `dragon_s_hoard` | Dragon's Hoard | functional definition |
| `druid_of_the_cowl` | Druid of the Cowl | functional definition |
| `dryad_greenseeker` | Dryad Greenseeker | functional definition |
| `duress` | Duress | functional definition |
| `dwarven_priest` | Dwarven Priest | functional definition |
| `electrify` | Electrify | functional definition |
| `elvish_clancaller` | Elvish Clancaller | functional definition |
| `elvish_rejuvenator` | Elvish Rejuvenator | functional definition |
| `enigma_drake` | Enigma Drake | functional definition |
| `epicure_of_blood` | Epicure of Blood | functional definition |
| `essence_scatter` | Essence Scatter | functional definition |
| `exclusion_mage` | Exclusion Mage | functional definition |
| `explosive_apparatus` | Explosive Apparatus | functional definition |
| `field_creeper` | Field Creeper | functional definition |
| `fiery_finish` | Fiery Finish | functional definition |
| `fire_elemental` | Fire Elemental | functional definition |
| `forest` | Forest | functional definition |
| `forsaken_sanctuary` | Forsaken Sanctuary | functional definition |
| `foul_orchard` | Foul Orchard | functional definition |
| `fountain_of_renewal` | Fountain of Renewal | functional definition |
| `fraying_omnipotence` | Fraying Omnipotence | functional definition |
| `frilled_sea_serpent` | Frilled Sea Serpent | functional definition |
| `gallant_cavalry` | Gallant Cavalry | functional definition |
| `gargoyle_sentinel` | Gargoyle Sentinel | functional definition |
| `gearsmith_guardian` | Gearsmith Guardian | functional definition |
| `gearsmith_prodigy` | Gearsmith Prodigy | functional definition |
| `ghastbark_twins` | Ghastbark Twins | functional definition |
| `ghirapur_guide` | Ghirapur Guide | functional definition |
| `ghostform` | Ghostform | functional definition |
| `giant_spider` | Giant Spider | functional definition |
| `gift_of_paradise` | Gift of Paradise | functional definition |
| `gigantosaurus` | Gigantosaurus | functional definition |
| `goblin_instigator` | Goblin Instigator | functional definition |
| `goblin_motivator` | Goblin Motivator | functional definition |
| `goblin_trashmaster` | Goblin Trashmaster | functional definition |
| `goreclaw_terror_of_qal_sisma` | Goreclaw, Terror of Qal Sisma | functional definition |
| `grasping_scoundrel` | Grasping Scoundrel | functional definition |
| `gravedigger` | Gravedigger | functional definition |
| `gravewaker` | Gravewaker | functional definition |
| `graveyard_marshal` | Graveyard Marshal | functional definition |
| `greenwood_sentinel` | Greenwood Sentinel | functional definition |
| `guttersnipe` | Guttersnipe | functional definition |
| `havoc_devils` | Havoc Devils | functional definition |
| `herald_of_faith` | Herald of Faith | functional definition |
| `heroic_reinforcements` | Heroic Reinforcements | functional definition |
| `highland_game` | Highland Game | functional definition |
| `highland_lake` | Highland Lake | functional definition |
| `hired_blade` | Hired Blade | functional definition |
| `horizon_scholar` | Horizon Scholar | functional definition |
| `hostile_minotaur` | Hostile Minotaur | functional definition |
| `infectious_horror` | Infectious Horror | functional definition |
| `infernal_reckoning` | Infernal Reckoning | functional definition |
| `infernal_scarring` | Infernal Scarring | functional definition |
| `inferno_hellion` | Inferno Hellion | functional definition |
| `inspired_charge` | Inspired Charge | functional definition |
| `invoke_the_divine` | Invoke the Divine | functional definition |
| `island` | Island | functional definition |
| `isolate` | Isolate | functional definition |
| `kargan_dragonrider` | Kargan Dragonrider | functional definition |
| `knight_of_the_tusk` | Knight of the Tusk | functional definition |
| `knight_s_pledge` | Knight's Pledge | functional definition |
| `knightly_valor` | Knightly Valor | functional definition |
| `lathliss_dragon_queen` | Lathliss, Dragon Queen | functional definition |
| `lava_axe` | Lava Axe | functional definition |
| `leonin_vanguard` | Leonin Vanguard | functional definition |
| `leonin_warleader` | Leonin Warleader | functional definition |
| `lich_s_caress` | Lich's Caress | functional definition |
| `lightning_strike` | Lightning Strike | functional definition |
| `liliana_s_spoils` | Liliana's Spoils | functional definition |
| `liliana_untouched_by_death` | Liliana, Untouched by Death | functional definition |
| `llanowar_elves` | Llanowar Elves | functional definition |
| `loxodon_line_breaker` | Loxodon Line Breaker | functional definition |
| `luminous_bonds` | Luminous Bonds | functional definition |
| `macabre_waltz` | Macabre Waltz | functional definition |
| `make_a_stand` | Make a Stand | functional definition |
| `manalith` | Manalith | functional definition |
| `marauder_s_axe` | Marauder's Axe | functional definition |
| `meandering_river` | Meandering River | functional definition |
| `mentor_of_the_meek` | Mentor of the Meek | functional definition |
| `meteor_golem` | Meteor Golem | functional definition |
| `mighty_leap` | Mighty Leap | functional definition |
| `militia_bugler` | Militia Bugler | functional definition |
| `millstone` | Millstone | functional definition |
| `mind_rot` | Mind Rot | functional definition |
| `mist_cloaked_herald` | Mist-Cloaked Herald | functional definition |
| `mistcaller` | Mistcaller | functional definition |
| `mountain` | Mountain | functional definition |
| `murder` | Murder | functional definition |
| `mystic_archaeologist` | Mystic Archaeologist | functional definition |
| `naturalize` | Naturalize | functional definition |
| `nicol_bolas_the_ravager` | Nicol Bolas, the Ravager | functional definition |
| `nightmare_s_thirst` | Nightmare's Thirst | functional definition |
| `novice_knight` | Novice Knight | functional definition |
| `oakenform` | Oakenform | functional definition |
| `omenspeaker` | Omenspeaker | functional definition |
| `onakke_ogre` | Onakke Ogre | functional definition |
| `one_with_the_machine` | One with the Machine | functional definition |
| `open_the_graves` | Open the Graves | functional definition |
| `oreskos_swiftclaw` | Oreskos Swiftclaw | functional definition |
| `palladia_mors_the_ruiner` | Palladia-Mors, the Ruiner | functional definition |
| `patient_rebuilding` | Patient Rebuilding | functional definition |
| `pelakka_wurm` | Pelakka Wurm | functional definition |
| `pendulum_of_patterns` | Pendulum of Patterns | functional definition |
| `plague_mare` | Plague Mare | functional definition |
| `plains` | Plains | functional definition |
| `plummet` | Plummet | functional definition |
| `poison_tip_archer` | Poison-Tip Archer | functional definition |
| `prodigious_growth` | Prodigious Growth | functional definition |
| `psychic_corrosion` | Psychic Corrosion | functional definition |
| `psychic_symbiont` | Psychic Symbiont | functional definition |
| `rabid_bite` | Rabid Bite | functional definition |
| `ravenous_harpy` | Ravenous Harpy | functional definition |
| `reassembling_skeleton` | Reassembling Skeleton | functional definition |
| `reclamation_sage` | Reclamation Sage | functional definition |
| `recollect` | Recollect | functional definition |
| `regal_bloodlord` | Regal Bloodlord | functional definition |
| `reliquary_tower` | Reliquary Tower | functional definition |
| `remorseful_cleric` | Remorseful Cleric | functional definition |
| `resplendent_angel` | Resplendent Angel | functional definition |
| `revitalize` | Revitalize | functional definition |
| `rhox_oracle` | Rhox Oracle | functional definition |
| `root_snare` | Root Snare | functional definition |
| `runic_armasaur` | Runic Armasaur | functional definition |
| `rustwing_falcon` | Rustwing Falcon | functional definition |
| `sai_master_thopterist` | Sai, Master Thopterist | functional definition |
| `salvager_of_secrets` | Salvager of Secrets | functional definition |
| `sarkhan_fireblood` | Sarkhan, Fireblood | functional definition |
| `sarkhan_s_dragonfire` | Sarkhan's Dragonfire | functional definition |
| `satyr_enchanter` | Satyr Enchanter | functional definition |
| `scapeshift` | Scapeshift | functional definition |
| `scholar_of_stars` | Scholar of Stars | functional definition |
| `serra_s_guardian` | Serra's Guardian | functional definition |
| `shivan_dragon` | Shivan Dragon | functional definition |
| `shock` | Shock | functional definition |
| `siegebreaker_giant` | Siegebreaker Giant | functional definition |
| `sift` | Sift | functional definition |
| `silverbeak_griffin` | Silverbeak Griffin | functional definition |
| `skalla_wolf` | Skalla Wolf | functional definition |
| `skeleton_archer` | Skeleton Archer | functional definition |
| `skymarch_bloodletter` | Skymarch Bloodletter | functional definition |
| `skyscanner` | Skyscanner | functional definition |
| `sleep` | Sleep | functional definition |
| `smelt` | Smelt | functional definition |
| `snapping_drake` | Snapping Drake | functional definition |
| `sovereign_s_bite` | Sovereign's Bite | functional definition |
| `spit_flame` | Spit Flame | functional definition |
| `stitcher_s_supplier` | Stitcher's Supplier | functional definition |
| `stone_quarry` | Stone Quarry | functional definition |
| `strangling_spores` | Strangling Spores | functional definition |
| `submerged_boneyard` | Submerged Boneyard | functional definition |
| `sun_sentinel` | Sun Sentinel | functional definition |
| `supreme_phantom` | Supreme Phantom | functional definition |
| `sure_strike` | Sure Strike | functional definition |
| `suspicious_bookcase` | Suspicious Bookcase | functional definition |
| `swamp` | Swamp | functional definition |
| `take_vengeance` | Take Vengeance | functional definition |
| `talons_of_wildwood` | Talons of Wildwood | functional definition |
| `tattered_mummy` | Tattered Mummy | functional definition |
| `tectonic_rift` | Tectonic Rift | functional definition |
| `tezzeret_artifice_master` | Tezzeret, Artifice Master | functional definition |
| `tezzeret_s_strider` | Tezzeret's Strider | functional definition |
| `thornhide_wolves` | Thornhide Wolves | functional definition |
| `thud` | Thud | functional definition |
| `timber_gorge` | Timber Gorge | functional definition |
| `titanic_growth` | Titanic Growth | functional definition |
| `tolarian_scholar` | Tolarian Scholar | functional definition |
| `tormenting_voice` | Tormenting Voice | functional definition |
| `totally_lost` | Totally Lost | functional definition |
| `tranquil_expanse` | Tranquil Expanse | functional definition |
| `trumpet_blast` | Trumpet Blast | functional definition |
| `trusty_packbeast` | Trusty Packbeast | functional definition |
| `two_headed_zombie` | Two-Headed Zombie | functional definition |
| `uncomfortable_chill` | Uncomfortable Chill | functional definition |
| `valiant_knight` | Valiant Knight | functional definition |
| `vampire_neonate` | Vampire Neonate | functional definition |
| `vampire_sovereign` | Vampire Sovereign | functional definition |
| `viashino_pyromancer` | Viashino Pyromancer | functional definition |
| `vigilant_baloth` | Vigilant Baloth | functional definition |
| `vine_mare` | Vine Mare | functional definition |
| `vivien_reid` | Vivien Reid | functional definition |
| `volcanic_dragon` | Volcanic Dragon | functional definition |
| `volley_veteran` | Volley Veteran | functional definition |
| `walking_corpse` | Walking Corpse | functional definition |
| `wall_of_mist` | Wall of Mist | functional definition |
| `wall_of_vines` | Wall of Vines | functional definition |
| `woodland_stream` | Woodland Stream | functional definition |

## Excluded (33)

Cards and mechanics considered and deliberately left out of scope, each with the blocker that keeps it out. Names and blockers only — no rules text. Curated by hand in `crates/sage-engine/data/exclusions.json`.

| Excluded | Blocker |
| --- | --- |
| Abilities that trigger on a mana ability being activated | the activation condition watches the objects a transition put on the stack, which a mana ability never reaches (CR 605.3a) |
| Abilities that trigger on someone else drawing a card | the draw trigger condition observes only its own controller's draws |
| Attack requirements | a block requirement is maximised over the whole declaration (CR 509.1c), but nothing can force a creature into the attacker declaration (CR 508.1d) — and the one requirement modeled is that every creature able to block an attacker does so, never that one particular creature blocks |
| Auras that enchant a player, or move between hosts | an Aura's enchant restriction is any class the target vocabulary names, so a creature and a land are both hosts, and its grant may be P/T, keywords, combat restrictions, or a written-out ability; but no attachment names a player, and once attached an Aura stays on the host it entered on — nothing moves one |
| Combat damage assigned by anything but the assigning creature's own power or toughness | an attacker or blocker assigns its current power, or its current toughness while a continuous effect names that one instead, read at the single place the combat-damage step asks how much a creature assigns; no other characteristic can be named, nothing assigns a fixed amount or a count, and nothing reads another object's characteristic |
| Conditions other than a permanent count, a mill, a discard, life gained this turn, or what one permanent has attacked, blocked, or damaged | a permanent count is a tally of a class and cannot require its members to have distinct names |
| Cost modification of another player's spells, or of an ability's activation cost | a permanent continuously takes generic mana off, or puts it on, the cost of a class of spell its own controller casts (CR 601.2f), read wherever a cast's cost is read; nothing reaches a spell another player casts, no modification applies to an activated ability's cost, and a coloured or colourless requirement is never changed |
| Costs paid by exiling from anywhere but a graveyard, or by choosing to pay at all | a cast and an activation each carry the sacrifices, discards, and graveyard exiles their cost names on the action — a fixed number of permanents or any number of them, and always the payer's own — but a cost exiles only out of the payer's own graveyard, never from a hand, a library, or the battlefield, and every non-mana cost is mandatory rather than an option the player may decline |
| Damage prevention beyond a blanket shield for the turn | a shield prevents all damage — or all combat damage — for the rest of the turn, consulted at the one seam damage is dealt, and a spell may declare its own damage unpreventable to defeat it; nothing prevents a fixed amount, names a recipient or a source, redirects damage, or lasts anything but the turn |
| Effects that ask a player to name a type | a permanent records the colour and the card its controller named as it entered — the card as a functional identity chosen from the catalog, never a string — but a card or creature type has no recorded identity, only a nonbasic land may be named, and nothing on a spell records a choice at all |
| Effects that let a player choose the order of the cards a scry keeps on top | a look bottoms its rest in an order the looker picks or at random, as the card says, but the cards a scry leaves on top stay in their printed order |
| Effects that return a card from a graveyard to a zone other than a hand or the battlefield | a targeted card returns from a graveyard to a hand or to the battlefield, and a whole graveyard can be exiled; nothing else moves a card out of one |
| Effects that untap a permanent it did not just take | untapping rides on the control change that steals a creature, because one effect names one target; nothing else brings an untap forward |
| Effects whose amount is derived from a source outside the ones the IR names | an amount may be a count of permanents (feeding power/toughness, life, damage, a token count, and an attachment's static grant), the life gained this turn, a count of what this resolution milled, the greatest mana value among a class of permanents, the X its controller announced, how many permanents this object's own cost sacrificed, the power the creature that cost sacrificed had, or half a named player's life total, hand, or creature count rounded up — the last seven feeding power/toughness, a draw, damage, a search's size, a life loss, a discard, and a sacrifice; a count of cards in a graveyard feeds a characteristic-defining power and nothing else, and a chosen permanent's power feeds only the life gained by the exile that removes it — nothing else may: not a whole life total, hand, or graveyard feeding an effect, not one named object's mana value, not a surviving object's power, not a chosen permanent's toughness or mana value, not half of anything rounded down, and not the permanent an effect just put onto the battlefield |
| Emblems with an activated ability | an emblem carries static and triggered abilities only; nothing offers a way to activate one |
| Equipment that grants a type, and cards that ask whether a creature is equipped | an attachment grants power/toughness, keywords, combat restrictions, and written-out abilities at CR 613 layers 6 and 7c — one block for both kinds, so an Equipment grants an ability exactly as an Aura does; it adds no type, and nothing asks whether a permanent is attached |
| Gaining control of a permanent for longer than a turn, and exchanging control | a control change is a targeted layer-2 effect the cleanup step ends; no duration outlives the turn and nothing swaps two permanents' controllers |
| Kicker and other optional additional costs | no optional cost declared on announcement |
| Losing abilities on a targeted permanent | a printed static ability may take all abilities from a class of permanents, and an until-end-of-turn removal names its own source and may lose named keywords; nothing takes abilities from a permanent an effect *targeted*, and no removal reaching another permanent has a duration shorter than its source's presence |
| Mana filtering | mana is produced and spent, never converted; nothing changes the colour of mana already in a pool |
| Modal double-faced cards, and melding | a card has an ordered list of faces and a permanent turns over between them (CR 712), but the second face is only ever reached by transforming: it carries no mana cost, the catalog validator refuses one, and no announcement offers a card as anything but its front face — so a card whose two faces are two things you may cast is unwritable, and nothing combines two cards into one |
| Modes beyond one chosen from a spell's printed list | a spell chooses exactly one of between two and four printed modes as it is announced, and the chosen mode alone decides which effects resolve and which targets are asked for; no ability is modal, nothing chooses two modes or repeats one, and a mode carries no cost of its own |
| Optional costs paid with anything but mana | an optional effect's cost is a mana payment; sacrificing, discarding, or exiling to pay is unwritable |
| Playing a card from a zone other than the hand, the command zone, or a permitted graveyard | a graveyard is reached three ways — a one-turn permission to cast from it, a continuous permission to play lands from it, and an activated or triggered ability that returns its own card out of it — but no other zone is reached at all: no per-turn exile permission, nothing played off the top of a library, no way to cast without paying a mana cost, and no alternative-cost or zone-specific casting mechanism (flashback, escape, adventure) |
| Protection | there is no protection layer: nothing stops a spell, a block, an aura, or damage by a quality the way CR 702.16 does |
| Reflexive triggers, and conditional branches that choose a target | an optional effect declares the target group of the one effect it wraps, but a conditional's branches, a wrapper over two targeting effects, and a "when you do" aimed after a cost is paid have no group one announcement could fill |
| Replacement effects other than one modifying a permanent entering the battlefield | the entering object's own self-replacements and a one-shot replacement an ability created for the turn are collected, ordered by the affected permanent's controller (CR 616.1), and applied once each (CR 614.5); damage is reached only by a prevention shield, and no other event can be replaced — not a permanent leaving the battlefield, a draw, or life gained — no permanent carries a static replacement ability, and the only substitution an entry can be given is exile |
| Rules that apply as though a permanent lacked a keyword other than defender | one as-though permission is modeled — attacking as though the creature did not have defender (CR 609.4), granted as a continuous effect that is in no layer and read only at the attacker declaration, so the keyword itself is untouched everywhere else; no other keyword can be ignored by a rule, and nothing applies as though a permanent had a keyword it does not |
| Selectors that filter by toughness, or by a power relative to another permanent's | a permanent count, an enters-or-dies trigger selector, a blocking restriction, a card choice, a mass-effect class, and the class of spell a cast trigger or a cost modifier names each carry a fixed power threshold; a target spec and a static ability's condition carry none, no threshold reads toughness, and no threshold is another permanent's power |
| Static abilities that affect a class of the source's controller's own noncreature permanents, or a class of tokens | the continuous-effect selector names the source, one class of that controller's creatures, or permanents an opponent controls filtered by card type and by the card name the source was given as it entered; it cannot name a class of the controller's own noncreature permanents, and nothing anywhere filters a class by token-ness |
| The legend-rule choice among duplicates | CR 704.5j applies, but which copy survives is a deterministic policy (the newest) rather than the controller's choice |
| Tokens created as copies of another permanent | there is no copiable-values model; a copy is decided at CR 613 layer 1, ahead of every layer the engine applies |
| X anywhere but a spell's own mana cost | a cast announces X, folds it into the cost as generic mana, and locks it on the stack for the resolution and the generated text to read; nothing announces X for an activation or on a trigger, and the announced value feeds only the amounts a derived amount already feeds — never the counters a permanent enters with, a token count, or a mana-value filter |
