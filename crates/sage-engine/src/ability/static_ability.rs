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
/// [`Effect::Conditional`] takes. Two of that enum's three variants ask what *this
/// resolution* has already done, and a continuous ability is not a resolution: it has no
/// window to read and no start to measure from, so those questions would be
/// unanswerable rather than merely unused.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
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
    /// **No layer**: the affected permanents may attack as though they did not have
    /// defender (CR 702.3b applied as though absent, CR 609.4).
    ///
    /// Not [`Modification::LoseKeyword`](crate::Modification::LoseKeyword): the creature
    /// keeps defender for every other purpose, including the `keyword` filter of the very
    /// selector that reached it. See [`RuleModification::AttacksAsThoughNoDefender`].
    AttacksAsThoughNoDefender,
}

impl StaticModification {
    /// The runtime [`Modification`](crate::Modification) this authored shape denotes.
    #[must_use]
    pub fn to_modification(self) -> crate::Modification {
        match self {
            StaticModification::PowerToughness { power, toughness } => {
                crate::Modification::PowerToughness { power, toughness }
            }
            StaticModification::GrantKeyword { keyword } => {
                crate::Modification::GrantKeyword(keyword)
            }
            StaticModification::AssignsCombatDamageBy { characteristic } => {
                crate::Modification::ModifyRule(RuleModification::AssignsCombatDamageBy {
                    characteristic,
                })
            }
            StaticModification::AttacksAsThoughNoDefender => {
                crate::Modification::ModifyRule(RuleModification::AttacksAsThoughNoDefender)
            }
        }
    }
}
