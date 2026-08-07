//! Static abilities (CR 604.3): which permanents one continuously modifies, and what
//! it does to them.

use super::*;

/// Which permanents a printed [`Ability::Static`] continuously modifies.
///
/// A closed, authored selector: it names a *class*, and is evaluated against each
/// permanent on demand relative to the ability's own source. Deliberately small —
/// it covers the anthem and lord shapes and grows by adding variants when a card
/// needs one ("creatures your opponents control", "permanents you control").
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum StaticAffects {
    /// Creatures controlled by the source's controller — "creatures you control".
    CreaturesYouControl {
        /// Restrict to creatures whose subtypes include this one, which is what
        /// makes a lord a lord (`Other **Elves** you control`). Absent means every
        /// creature its controller controls.
        #[serde(default)]
        subtype: Option<String>,
        /// Exclude the source itself — the "other" in "other Elves you control".
        /// A lord that pumped itself would be a different card.
        #[serde(default)]
        except_this: bool,
        /// Restrict to creatures that have this keyword — the `with **defender**` of
        /// "each creature you control with defender". Absent means every creature.
        ///
        /// Read off the **printed** face, exactly as `subtype` beside it is, and for the
        /// same reason: this selector is evaluated from inside the computation of the
        /// affected permanent's characteristics, and the computed keyword set is what
        /// that computation is producing. So a creature *granted* defender is outside the
        /// class, which is the same limitation the printed subtype read already carries.
        /// The observer's counterpart
        /// ([`ObservedPermanent`](crate::ObservedPermanent)) is evaluated outside the
        /// layer system and does read the computed keywords — the asymmetry is the
        /// recursion, not a difference of opinion.
        #[serde(default)]
        keyword: Option<Keyword>,
    },
    /// The source permanent and nothing else — the "this creature" of `This creature
    /// gets +1/+0 as long as you control an artifact.`
    ///
    /// A class of one, and deliberately a class rather than a special case: it flows
    /// through the same selector, the same timestamp, and the same layer as an anthem,
    /// so a self-modifying static ability needs no path of its own. An emblem is not a
    /// permanent and so has no source to affect; a `source` static on one applies to
    /// nothing, which is the honest answer rather than a panic.
    Source,
    /// The permanent this source is **attached to** — the "enchanted creature" of an Aura
    /// and the "equipped creature" of an Equipment (CR 303.4, CR 301.5).
    ///
    /// A class of one, like [`Self::Source`], and the other half of what an attachment
    /// grants. [`Attachment`](crate::Attachment)'s own fields cover the grants a card
    /// prints most often — power and toughness, keywords, combat restrictions — and this
    /// covers the ones they cannot: a static ability of the *attachment* that speaks
    /// about its host.
    ///
    /// It is deliberately not the same thing as putting the ability in
    /// [`Attachment::abilities`](crate::Attachment), which **grants** it to the host. The
    /// difference is whose ability it is, and it is visible: a creature that loses all
    /// abilities loses a granted one and keeps this one, because this one was never its.
    /// Waterknot's `enchanted creature doesn't untap` is the Aura's sentence about the
    /// creature, not the creature's sentence about itself.
    ///
    /// Applies to nothing while the source is attached to nothing, and nothing at all for
    /// an emblem, which can be attached to nothing at all.
    AttachedTo,
    /// Permanents controlled by an **opponent** of the source's controller — the first
    /// class a static ability names that its own controller does not control.
    ///
    /// The mirror of [`Self::CreaturesYouControl`], and relative to the source's
    /// controller for the same reason: one authored card must mean "your opponents" from
    /// either side of the table. It is not restricted to creatures — that restriction was
    /// the whole of the old selector's reach — so `card_type` is how a card says which
    /// permanents it means.
    ///
    /// Like every other selector it is **re-derived on every read**, which is the entire
    /// distinction between a static ability and the resolution-time
    /// [`MassAffects`](crate::MassAffects) class a sweeper names: a land that arrives
    /// under an opponent after the source did is affected the instant it arrives, one
    /// that changes hands leaves the class at CR 613 layer 2, and everything stops the
    /// instant the source leaves the battlefield.
    PermanentsYourOpponentsControl {
        /// Restrict to permanents with this printed card type — the `land` of "lands
        /// your opponents control". Absent means every permanent they control.
        #[serde(default)]
        card_type: Option<CardType>,
        /// Restrict to permanents whose printed card is the one the **source named as it
        /// entered** (CR 614.12) — the "with the chosen name" of a card that asks its
        /// controller to name one.
        ///
        /// A source that named nothing matches nothing, which is the honest reading:
        /// there is no chosen name, so no permanent has it. A token has no card at all
        /// (CR 111) and so can never carry a name that was named.
        #[serde(default)]
        with_the_named_card: bool,
    },
    /// Every **creature token** on the battlefield, whoever controls it — Amulet of
    /// Safekeeping's `creature tokens get -1/-0`.
    ///
    /// The first class here that is symmetric: it reaches its own controller's tokens and
    /// an opponent's alike, because the card names none of them. It is also the first that
    /// filters by what a permanent *is* rather than by what it is printed as — a token has
    /// no card (CR 111), which the state already records
    /// ([`Printed::is_token`](crate::Printed::is_token)), so this is read off the object
    /// rather than inferred from a missing card handle.
    EachCreatureToken,
}

/// What has to be true for a printed [`Ability::Static`] to be **in force** — the
/// `as long as …` of a conditional continuous ability.
///
/// Absent from an ability means unconditional, which is what every anthem and lord
/// says. Present, it is re-asked on **every read** of the affected permanent's
/// characteristics, exactly as the static ability itself is derived on every read
/// (ADR 0005 §1): a creature that gets +2/+0 while you control an artifact loses it the
/// instant the artifact does, with nothing to prune and no way for the modifier to
/// outlive the condition.
///
/// Deliberately a small enum of its own rather than the [`Condition`] an
/// [`Effect::Conditional`] takes. Most of that enum reads events over a window — this
/// resolution, or this turn — and a continuous ability is neither: it has no resolution
/// to measure from and no reason to stop at a turn boundary, so those questions would be
/// unanswerable here rather than merely unused. What the two vocabularies share is
/// asked through one reader, never two ([`crate::condition::count_permanents`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StaticCondition {
    /// The source's controller controls at least `count` permanents matching
    /// `permanents` — `as long as you control an artifact`, `as long as you control an
    /// Ajani planeswalker`.
    ControlsAtLeast {
        /// Which permanents are counted, relative to the source's controller.
        permanents: crate::ability::PermanentCount,
        /// The threshold, inclusive. Defaults to one, which is what "as long as you
        /// control **an** artifact" means.
        #[serde(default = "one_permanent")]
        count: u32,
    },
    /// The source permanent is currently **attacking** — `as long as it's attacking`.
    ///
    /// Read off the declaration the combat step produced, so it turns on when attackers
    /// are declared and off when the permanent leaves combat, with no event to observe
    /// in either direction.
    SourceIsAttacking,
    /// The source permanent currently has something **attached** to it — `as long as
    /// this creature is enchanted or equipped`.
    ///
    /// One condition rather than two, because the card prints one: an Aura and an
    /// Equipment are the same fact about the host (CR 303.4 / CR 301.5 — something is
    /// attached to it), and only the attachment's own kind tells them apart. Read off
    /// [`Permanent::attached_to`](crate::Permanent::attached_to) on every read, so it
    /// turns on the instant an Aura resolves onto the creature and off the instant the
    /// Equipment is moved elsewhere, with nothing to prune either way.
    SourceIsEnchantedOrEquipped,
    /// The source permanent **has not yet dealt damage** — `as long as it hasn't dealt
    /// damage yet` (CR 120).
    ///
    /// The window is the object's whole life, not a turn and not a resolution, which is
    /// why this is here and not in [`Condition`]: "yet" reaches back to the moment the
    /// permanent entered the battlefield, and a continuous ability has no resolution to
    /// measure anything from. It is also why the answer is a fact stored on the permanent
    /// ([`Permanent::dealt_damage`](crate::Permanent)) rather than read out of the event
    /// log the turn-scoped conditions read: the log is a bounded ring, so a game long
    /// enough would forget the very hit this condition exists to notice.
    ///
    /// Stored, but never *latched*: the flag is a fact about the permanent, and the
    /// question is re-asked on every read of any permanent's characteristics, so hexproof
    /// disappears in the same batch the damage lands in rather than at the next
    /// resolution. A permanent that leaves and returns is a new object with a fresh
    /// [`PermanentId`](crate::PermanentId) and a fresh answer, which is what CR 400.7
    /// means here.
    SourceHasNotDealtDamage,
}

/// The default threshold of a [`StaticCondition::ControlsAtLeast`]: one, the "an" of
/// "as long as you control **an** artifact".
fn one_permanent() -> u32 {
    1
}

/// What a printed [`Ability::Static`] does to the permanents it affects. The
/// variant fixes the CR 613 layer, exactly as the runtime
/// [`Modification`](crate::Modification) it maps to does.
///
/// This is the *authored* shape and is deliberately separate from the runtime enum,
/// the same seam [`TargetSpec`] keeps from [`Target`]: the JSON a card is written in
/// must not shift because an internal representation changed.
///
/// `Clone` rather than `Copy`, following its [`Modification`](crate::Modification): a
/// grant of a whole written-out ability carries that ability's entire tree.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StaticModification {
    /// CR 613 **layer 7c**: add the given signed amounts to power and toughness.
    PowerToughness {
        /// Amount added to power; negative subtracts.
        power: i32,
        /// Amount added to toughness; negative subtracts.
        toughness: i32,
    },
    /// CR 613 **layer 6** (CR 613.1f): grant a keyword ability. Redundant grants are
    /// idempotent (CR 702.2c), so an anthem granting flying to a creature that already
    /// flies changes nothing.
    GrantKeyword {
        /// The keyword granted for as long as the source is on the battlefield.
        keyword: Keyword,
    },
    /// **No layer**: the affected permanents assign combat damage equal to the named
    /// characteristic rather than to their power (CR 510.1a, modified).
    ///
    /// Not a power-setting effect, and the distinction is the card. The creature's power
    /// is untouched — a 0/5 Wall under Arcades is still a 0/5 to every selector, every
    /// evasion rule, and the projected view — and only the combat-damage step reads the
    /// substitute. See [`RuleModification::AssignsCombatDamageBy`].
    AssignsCombatDamageBy {
        /// Which of the creature's own characteristics the amount comes from.
        characteristic: DamageCharacteristic,
    },
    /// **No layer**: the affected permanents do not untap during their controller's
    /// untap step (CR 502.4, modified). The `enchanted creature doesn't untap` of an Aura
    /// that holds a creature down.
    DoesNotUntap,
    /// **No layer**: the affected permanents may attack as though they did not have
    /// defender (CR 702.3b applied as though absent, CR 609.4).
    ///
    /// Not [`Modification::LoseKeyword`](crate::Modification::LoseKeyword): the creature
    /// keeps defender for every other purpose, including the `keyword` filter of the very
    /// selector that reached it. See [`RuleModification::AttacksAsThoughNoDefender`].
    AttacksAsThoughNoDefender,
    /// CR 613 **layer 6** (CR 613.1f): the affected permanents **lose all abilities** —
    /// every keyword, every combat restriction, and every printed static, triggered, and
    /// activated ability.
    ///
    /// The subtracting member of this vocabulary, and the reason a printed static ability
    /// now reaches the ability fold at all: until one could remove abilities, the only
    /// removals were the until-end-of-turn kind an effect stored, and the fold could
    /// safely ignore this source list. Ordered by timestamp like everything else in the
    /// layer, so a grant with a later timestamp — an Aura hung on the silenced permanent
    /// afterwards — still grants.
    LoseAllAbilities,
    /// CR 613 **layer 6** (CR 613.1f): grant a **written-out ability** to the affected
    /// permanents — the `have "{T}: Add one mana of any color."` of a card that takes a
    /// land's own abilities away and leaves it able to tap for something.
    ///
    /// The same grant an attachment makes, reached from a printed static ability instead
    /// of from an Aura, and folded in by the same
    /// [`current_abilities`](crate::characteristics::current_abilities) — so the granted
    /// ability is offered by [`valid_actions`](crate::valid_actions), a granted mana
    /// ability still uses no stack (CR 605.1a), and a granted trigger is collected, each
    /// by the code a printed ability goes through.
    GrantAbility {
        /// The ability granted for as long as the source is on the battlefield. Boxed
        /// for the reason [`Modification::GrantAbility`](crate::Modification) is: an
        /// [`Ability`] can contain one of these, and an unboxed cycle has no size.
        ability: Box<Ability>,
    },
}

/// What a printed [`Ability::CostModifier`] does to the cost of casting a spell
/// (CR 601.2f).
///
/// Deliberately **generic mana only**, in both directions. A cost modification changes
/// the generic component and leaves every coloured and `{C}` requirement exactly as
/// printed, which is what every printed reducer and tax in this set says; a card that
/// removed a coloured pip would be saying something else, and would add a variant here.
///
/// Two variants rather than one signed amount, because the two are applied at different
/// moments (CR 601.2f: the total is the printed cost *plus* every increase, *minus* every
/// reduction, and the mana component can never fall below `{0}`). A signed amount would
/// make that ordering an accident of summation instead of a rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CostModification {
    /// The spell costs this much **less** generic mana — "costs {2} less to cast".
    Reduce {
        /// How much generic mana comes off. Never takes the cost below `{0}`
        /// (CR 601.2f).
        generic: u8,
    },
    /// The spell costs this much **more** generic mana — the tax half, "costs {1} more
    /// to cast".
    Increase {
        /// How much generic mana goes on.
        generic: u8,
    },
}

impl StaticModification {
    /// The runtime [`Modification`](crate::Modification) this authored shape denotes.
    #[must_use]
    pub fn to_modification(&self) -> crate::Modification {
        match self {
            StaticModification::PowerToughness { power, toughness } => {
                crate::Modification::PowerToughness {
                    power: *power,
                    toughness: *toughness,
                }
            }
            StaticModification::GrantKeyword { keyword } => {
                crate::Modification::GrantKeyword(*keyword)
            }
            StaticModification::LoseAllAbilities => crate::Modification::LoseAllAbilities,
            StaticModification::GrantAbility { ability } => {
                crate::Modification::GrantAbility(ability.clone())
            }
            StaticModification::AssignsCombatDamageBy { characteristic } => {
                crate::Modification::ModifyRule(RuleModification::AssignsCombatDamageBy {
                    characteristic: *characteristic,
                })
            }
            StaticModification::AttacksAsThoughNoDefender => {
                crate::Modification::ModifyRule(RuleModification::AttacksAsThoughNoDefender)
            }
            StaticModification::DoesNotUntap => {
                crate::Modification::ModifyRule(RuleModification::DoesNotUntap)
            }
        }
    }
}
