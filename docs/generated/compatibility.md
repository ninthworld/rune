<!-- @generated — do not edit by hand.
     Regenerate with `make compat` (or `cargo run -p sage-engine --bin gen-compat`).
     `cargo test` fails if this file drifts from the catalog or the exclusion list.
     Source: crates/sage-engine/data/catalog/ + crates/sage-engine/data/exclusions.json (issue #258). -->

# Card compatibility report

SAGE supports only the verified slice of cards in its catalog, never a full set. This report is generated from the catalog and the curated exclusion list — the checkable artifact behind that claim (issue #258).

## Supported cards (197)

Every functional definition in `crates/sage-engine/data/catalog/`, in interned order. "Implementation" is whether the card's behavior lives in its data definition or (also) in the `scripted` code escape hatch (ADR 0008 §2).

| Functional ID | Name | Implementation |
| --- | --- | --- |
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
| `angel_of_the_dawn` | Angel of the Dawn | functional definition |
| `arcane_encyclopedia` | Arcane Encyclopedia | functional definition |
| `arisen_gorgon` | Arisen Gorgon | functional definition |
| `aven_wind_mage` | Aven Wind Mage | functional definition |
| `aviation_pioneer` | Aviation Pioneer | functional definition |
| `befuddle` | Befuddle | functional definition |
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
| `colossal_dreadmaw` | Colossal Dreadmaw | functional definition |
| `colossal_majesty` | Colossal Majesty | functional definition |
| `court_cleric` | Court Cleric | functional definition |
| `crash_through` | Crash Through | functional definition |
| `daggerback_basilisk` | Daggerback Basilisk | functional definition |
| `daybreak_chaplain` | Daybreak Chaplain | functional definition |
| `death_baron` | Death Baron | functional definition |
| `demon_of_catastrophes` | Demon of Catastrophes | functional definition |
| `diregraf_ghoul` | Diregraf Ghoul | functional definition |
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
| `frilled_sea_serpent` | Frilled Sea Serpent | functional definition |
| `gallant_cavalry` | Gallant Cavalry | functional definition |
| `gearsmith_guardian` | Gearsmith Guardian | functional definition |
| `gearsmith_prodigy` | Gearsmith Prodigy | functional definition |
| `ghirapur_guide` | Ghirapur Guide | functional definition |
| `giant_spider` | Giant Spider | functional definition |
| `gigantosaurus` | Gigantosaurus | functional definition |
| `goblin_instigator` | Goblin Instigator | functional definition |
| `goblin_motivator` | Goblin Motivator | functional definition |
| `grasping_scoundrel` | Grasping Scoundrel | functional definition |
| `gravedigger` | Gravedigger | functional definition |
| `gravewaker` | Gravewaker | functional definition |
| `greenwood_sentinel` | Greenwood Sentinel | functional definition |
| `guttersnipe` | Guttersnipe | functional definition |
| `havoc_devils` | Havoc Devils | functional definition |
| `herald_of_faith` | Herald of Faith | functional definition |
| `heroic_reinforcements` | Heroic Reinforcements | functional definition |
| `highland_game` | Highland Game | functional definition |
| `highland_lake` | Highland Lake | functional definition |
| `horizon_scholar` | Horizon Scholar | functional definition |
| `hostile_minotaur` | Hostile Minotaur | functional definition |
| `infectious_horror` | Infectious Horror | functional definition |
| `inspired_charge` | Inspired Charge | functional definition |
| `invoke_the_divine` | Invoke the Divine | functional definition |
| `island` | Island | functional definition |
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
| `meandering_river` | Meandering River | functional definition |
| `mentor_of_the_meek` | Mentor of the Meek | functional definition |
| `meteor_golem` | Meteor Golem | functional definition |
| `mighty_leap` | Mighty Leap | functional definition |
| `militia_bugler` | Militia Bugler | functional definition |
| `millstone` | Millstone | functional definition |
| `mind_rot` | Mind Rot | functional definition |
| `mist_cloaked_herald` | Mist-Cloaked Herald | functional definition |
| `mountain` | Mountain | functional definition |
| `murder` | Murder | functional definition |
| `mystic_archaeologist` | Mystic Archaeologist | functional definition |
| `naturalize` | Naturalize | functional definition |
| `oakenform` | Oakenform | functional definition |
| `omenspeaker` | Omenspeaker | functional definition |
| `onakke_ogre` | Onakke Ogre | functional definition |
| `open_the_graves` | Open the Graves | functional definition |
| `oreskos_swiftclaw` | Oreskos Swiftclaw | functional definition |
| `pelakka_wurm` | Pelakka Wurm | functional definition |
| `pendulum_of_patterns` | Pendulum of Patterns | functional definition |
| `plague_mare` | Plague Mare | functional definition |
| `plains` | Plains | functional definition |
| `plummet` | Plummet | functional definition |
| `poison_tip_archer` | Poison-Tip Archer | functional definition |
| `prodigious_growth` | Prodigious Growth | functional definition |
| `psychic_corrosion` | Psychic Corrosion | functional definition |
| `psychic_symbiont` | Psychic Symbiont | functional definition |
| `reclamation_sage` | Reclamation Sage | functional definition |
| `recollect` | Recollect | functional definition |
| `regal_bloodlord` | Regal Bloodlord | functional definition |
| `reliquary_tower` | Reliquary Tower | functional definition |
| `remorseful_cleric` | Remorseful Cleric | functional definition |
| `resplendent_angel` | Resplendent Angel | functional definition |
| `revitalize` | Revitalize | functional definition |
| `rhox_oracle` | Rhox Oracle | functional definition |
| `runic_armasaur` | Runic Armasaur | functional definition |
| `rustwing_falcon` | Rustwing Falcon | functional definition |
| `salvager_of_secrets` | Salvager of Secrets | functional definition |
| `sarkhan_fireblood` | Sarkhan, Fireblood | functional definition |
| `sarkhan_s_dragonfire` | Sarkhan's Dragonfire | functional definition |
| `satyr_enchanter` | Satyr Enchanter | functional definition |
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
| `tattered_mummy` | Tattered Mummy | functional definition |
| `tectonic_rift` | Tectonic Rift | functional definition |
| `tezzeret_artifice_master` | Tezzeret, Artifice Master | functional definition |
| `tezzeret_s_strider` | Tezzeret's Strider | functional definition |
| `thornhide_wolves` | Thornhide Wolves | functional definition |
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
| Activation costs paid by sacrificing another permanent, or by discarding | a cast carries its chosen sacrifices and discards on the action, but an activation names only its source, its ability, and its targets |
| Attack and block requirements, and blocking an additional creature | a declaration can be restricted but never required, and a blocker blocks one attacker |
| Auras that enchant a player or land, or move between hosts | only enchant-creature Auras granting P/T, keywords, or combat restrictions are modeled |
| Casting from a zone other than the hand, the command zone, or a one-turn graveyard permission | no alternative-cost or zone-specific casting mechanism (flashback, escape, adventure) |
| Combat damage assigned by a value other than power | every attacker and blocker assigns damage equal to its current power |
| Conditions other than a permanent count, a mill, a discard, or life gained this turn | a permanent count cannot require distinct names, and no condition asks what one permanent has done — on the intervening-if side or the continuous one |
| Cost reduction and cost increase effects | no cost-modification layer |
| Damage prevention | no prevention shield or damage-replacement layer |
| Effects that ask a player to name a colour, type, or card | no player choice recorded on a permanent or spell |
| Effects that let a player choose the order of cards put back on a library | a scry keeps its unchosen cards in their printed order and a look bottoms its rest at random |
| Effects that return a card from a graveyard to a zone other than a hand or the battlefield | a targeted card returns from a graveyard to a hand or to the battlefield, and a whole graveyard can be exiled; nothing else moves a card out of one |
| Effects that untap a permanent it did not just take | untapping rides on the control change that steals a creature, because one effect names one target; nothing else brings an untap forward |
| Effects whose amount is derived from anything but a count of permanents | an amount may scale with a count of permanents — power/toughness, life, or damage; cards in a zone, life totals, and mana values feed nothing |
| Emblems with an activated ability | an emblem carries static and triggered abilities only; nothing offers a way to activate one |
| Equipment | no equip action or attachment outside the Aura model |
| Fight, and other effects taking two differently-specified targets | one effect's target slots all share a single spec, so two differently-specified slots are unwritable |
| Gaining control of a permanent for longer than a turn, and exchanging control | a control change is a targeted layer-2 effect the cleanup step ends; no duration outlives the turn and nothing swaps two permanents' controllers |
| Keyword removal and loses-all-abilities effects | the ability-adding layer only adds abilities |
| Kicker and other optional additional costs | no optional cost declared on announcement |
| Mana filtering | mana is produced and spent, never converted; nothing changes the colour of mana already in a pool |
| Modal spells that choose one | no mode choice on announcement |
| Multi-face cards (transform, modal double-faced) | the card model has a single face |
| Optional costs paid with anything but mana | an optional effect's cost is a mana payment; sacrificing, discarding, or exiling to pay is unwritable |
| Protection, and evasion that names a subtype or a land type | a blocking restriction names a colour, a count, or a power; there is no protection layer |
| Reflexive triggers, and conditional branches that choose a target | an optional effect declares the target group of the one effect it wraps, but a conditional's branches, a wrapper over two targeting effects, and a "when you do" aimed after a cost is paid have no group one announcement could fill |
| Replacement effects | no replacement-effect layer in the rules engine |
| Selectors that filter by toughness, or by a power relative to another permanent's | a permanent count, an enters-or-dies trigger selector, a blocking restriction, and a card choice each name a fixed power threshold; a target spec, a mass-effect class, and a static ability's condition name none, no threshold reads toughness, and no threshold is another permanent's power |
| Spells with X in their cost | mana costs are fixed strings with no X announcement |
| Static abilities that affect anything but the source or creatures its controller controls | the continuous-effect selector names the source or one class of that controller's creatures, so a permanent or an emblem may modify no other |
| The legend-rule choice among duplicates | CR 704.5j applies, but which copy survives is a deterministic policy (the newest) rather than the controller's choice |
| Tokens created as copies of another permanent | there is no copiable-values model; a copy is decided at CR 613 layer 1, ahead of every layer the engine applies |
