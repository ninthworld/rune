//! Every way a catalog definition can be malformed, and how each one reads.
//!
//! Split out of `catalog.rs` for size (issue #711). Like its parent this names no
//! `crate::` path: `build.rs` compiles it before the engine exists.

use super::*;

/// A catalog file that does not satisfy the authored schema (ADR 0008 §5).
///
/// Returned by [`validate_definition`] and [`check_printings`]. `build.rs` turns one
/// of these into a build failure; the loader turns it into a
/// [`CatalogError`](crate::CatalogError). Each variant names the offending card so the
/// message points at the file to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    /// A definition file holds something other than a single JSON object — most
    /// likely the old monolithic array, which ADR 0008 §4 replaced with one file per
    /// card.
    NotAnObject,
    /// A required field is missing, or holds the wrong JSON type.
    MalformedField {
        /// The card the field belongs to, or the file stem if identity itself is the
        /// problem.
        functional_id: String,
        /// The field at fault.
        field: &'static str,
    },
    /// A definition declares a `schema_version` this engine does not understand.
    UnsupportedSchemaVersion {
        /// The definition that declared it.
        functional_id: String,
        /// The version it declared.
        found: u64,
    },
    /// A `functional_id` is not a well-formed slug (see [`is_well_formed_slug`]).
    MalformedFunctionalId {
        /// The ill-formed slug.
        slug: String,
    },
    /// A definition's `functional_id` does not match the file it is stored in. The
    /// file name *is* the identity (ADR 0008 §4), so the two may not disagree.
    FileNameMismatch {
        /// The identity the file declares.
        functional_id: String,
        /// The file it was found in, without its `.json` extension.
        file_stem: String,
    },
    /// A `Creature` carries no printed power/toughness, or a non-creature carries
    /// them (ADR 0008 §5).
    PowerToughnessMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is a creature — which is to say, which way it is wrong.
        creature: bool,
    },
    /// A `Planeswalker` carries no printed `loyalty`, or a non-planeswalker carries
    /// one (CR 306.5b) — the planeswalker counterpart of
    /// [`Self::PowerToughnessMismatch`].
    ///
    /// Wrong in both directions for the same reason that one is: a planeswalker with no
    /// starting loyalty would enter the battlefield with no loyalty counters and be put
    /// straight into its owner's graveyard by CR 704.5i, and a loyalty on anything else
    /// is a number nothing would ever read.
    LoyaltyMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the card is a planeswalker — which is to say, which way it is wrong.
        planeswalker: bool,
    },
    /// An `aura` grant appears on a card whose `subtypes` do not include `"Aura"`
    /// (CR 303.4).
    AuraOnNonAura {
        /// The definition at fault.
        functional_id: String,
    },
    /// An `additional_cost` appears on a card that cannot be cast, or names a cost of
    /// nothing. A land is *played*, not cast (CR 116.2a), so a cast cost on one could
    /// never be paid or checked; a cost of zero cards is not a cost, and authoring one
    /// is a way of writing a card that reads as costing something and does not.
    AdditionalCostIsUnpayable {
        /// The definition at fault.
        functional_id: String,
    },
    /// A printed `restrictions` list appears on a card that is not a creature. Every
    /// combat restriction is about attacking or blocking (CR 506.3, CR 509.1b), so on
    /// a non-creature it could only ever be inert — which makes it an authoring
    /// mistake worth failing on rather than a harmless field.
    RestrictionsOnNonCreature {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_token` effect describes a token that could not be a permanent: it
    /// names no types at all, or names one no permanent has (an instant or a sorcery).
    ///
    /// The battlefield is the only zone a token may be in (CR 111.7), so a token that
    /// is not a permanent could never exist — the card would author an object with
    /// nowhere to go, which is worth failing the build over rather than creating.
    TokenIsNotAPermanent {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_token` effect describes a creature token with no power/toughness, or
    /// a noncreature token that carries them — the token counterpart of
    /// [`Self::PowerToughnessMismatch`], and wrong for the same reason.
    TokenPowerToughnessMismatch {
        /// The definition at fault.
        functional_id: String,
        /// Whether the token is a creature — which is to say, which way it is wrong.
        creature: bool,
    },
    /// An optional effect (`{"kind":"may"}`) wraps **two** effects that choose a
    /// target.
    ///
    /// A `may` declares the target group of the one effect it wraps, so the slot is
    /// named at announcement (CR 601.2c) and the yes-or-no comes on resolution. One
    /// effect still declares at most one group, so a wrapper over two of them would
    /// have to advertise two slots from one forwarding and could never pair the flat
    /// stored target list back onto them. That is a card that looks authored and is
    /// not, which is worth failing the build over.
    TwoTargetsInsideOptional {
        /// The definition at fault.
        functional_id: String,
    },
    /// A conditional effect (`{"kind":"conditional"}`) has a branch that chooses a
    /// target.
    ///
    /// The neighbouring rule to [`Self::TwoTargetsInsideOptional`], and the reason a
    /// conditional forwards nothing where an optional effect forwards one group: its
    /// two branches share one flat target list, so a group named in either could not be
    /// paired back onto the branch that was actually taken. A branch that targeted
    /// would have its slot filled by nobody at announcement (CR 601.2c) and silently do
    /// nothing.
    TargetInsideConditional {
        /// The definition at fault.
        functional_id: String,
    },
    /// A **static** ability's `as long as …` condition counts permanents by
    /// `min_power`.
    ///
    /// A power bound is the one selector field read through the computed
    /// characteristics rather than the printed face, because power is what the
    /// implemented layers actually change. That reading is correct everywhere a
    /// condition is evaluated during a resolution, and non-terminating in the one place
    /// it is evaluated *inside* the layer system: computing a permanent's
    /// characteristics would ask for another permanent's characteristics, which would
    /// ask again. Rejected at build time rather than left as a stack overflow nobody
    /// can read.
    PowerInStaticCondition {
        /// The definition at fault.
        functional_id: String,
    },
    /// A `create_emblem` effect gives the emblem an ability an emblem cannot have
    /// (CR 114.1–114.4) — anything but a static or triggered ability.
    ///
    /// An emblem is in no zone and is never an object a player acts on, so an activated
    /// ability would have no way to be activated and an enters-the-battlefield
    /// self-replacement would have no entry to replace. Either is an emblem with a dead
    /// ability, which is worth failing the build over.
    EmblemAbilityIsNotStaticOrTriggered {
        /// The definition at fault.
        functional_id: String,
    },
    /// One ability or spell declares **two** variable-arity target groups (two effects
    /// each taking "up to N" targets).
    ///
    /// Targets are stored as one flat list per stack object, and the pairing back onto
    /// effects works by giving every fixed group its size and the slack to the one
    /// variable group ([`target_counts`](crate::target_counts)). With two variable
    /// groups the split is ambiguous — six targets could be four and two or two and four
    /// — and no announcement could say which. One card wants this shape and none wants
    /// two, so the ambiguity is rejected rather than guessed at.
    TwoVariableTargetGroups {
        /// The definition at fault.
        functional_id: String,
    },
    /// Two printings in one set claim the same collector number, so one would shadow
    /// the other.
    DuplicatePrinting {
        /// The set the collision is in.
        set_code: String,
        /// The collector number claimed twice.
        collector_number: String,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAnObject => write!(
                f,
                "a functional definition must be a single JSON object, one card per file"
            ),
            Self::MalformedField {
                functional_id,
                field,
            } => write!(f, "{functional_id}: `{field}` is missing or the wrong type"),
            Self::UnsupportedSchemaVersion {
                functional_id,
                found,
            } => write!(
                f,
                "{functional_id} declares schema_version {found}; \
                 this engine understands {SCHEMA_VERSION}"
            ),
            Self::MalformedFunctionalId { slug } => write!(
                f,
                "`{slug}` is not a well-formed functional id: expected a lowercase \
                 snake_case slug (e.g. `onakke_ogre`)"
            ),
            Self::FileNameMismatch {
                functional_id,
                file_stem,
            } => write!(
                f,
                "{file_stem}.json declares functional_id `{functional_id}`; \
                 a definition's file name must match its identity"
            ),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: true,
            } => write!(f, "{functional_id} is a Creature with no power/toughness"),
            Self::PowerToughnessMismatch {
                functional_id,
                creature: false,
            } => write!(
                f,
                "{functional_id} is not a Creature but carries power/toughness"
            ),
            Self::LoyaltyMismatch {
                functional_id,
                planeswalker: true,
            } => write!(f, "{functional_id} is a Planeswalker with no loyalty"),
            Self::LoyaltyMismatch {
                functional_id,
                planeswalker: false,
            } => write!(
                f,
                "{functional_id} is not a Planeswalker but carries loyalty"
            ),
            Self::AuraOnNonAura { functional_id } => write!(
                f,
                "{functional_id} carries an `aura` grant but is not an Aura \
                 (its subtypes do not include `{AURA_SUBTYPE}`)"
            ),
            Self::AdditionalCostIsUnpayable { functional_id } => write!(
                f,
                "{functional_id} carries an `additional_cost` that could never be paid: \
                 a land is played rather than cast, and a cost of no cards is no cost"
            ),
            Self::RestrictionsOnNonCreature { functional_id } => write!(
                f,
                "{functional_id} carries printed `restrictions` but is not a creature; \
                 a combat restriction can only restrict attacking or blocking"
            ),
            Self::TokenIsNotAPermanent { functional_id } => write!(
                f,
                "{functional_id} creates a token that is not a permanent; \
                 a token exists only on the battlefield, so its `types` must be \
                 permanent types"
            ),
            Self::TokenPowerToughnessMismatch {
                functional_id,
                creature: true,
            } => write!(
                f,
                "{functional_id} creates a Creature token with no power/toughness"
            ),
            Self::TokenPowerToughnessMismatch {
                functional_id,
                creature: false,
            } => write!(
                f,
                "{functional_id} creates a token that is not a Creature \
                 but carries power/toughness"
            ),
            Self::TwoTargetsInsideOptional { functional_id } => write!(
                f,
                "{functional_id} has a `may` effect wrapping two effects that target; \
                 an optional effect declares the target group of one wrapped effect"
            ),
            Self::TargetInsideConditional { functional_id } => write!(
                f,
                "{functional_id}: a conditional effect's branch may not choose a target"
            ),
            Self::PowerInStaticCondition { functional_id } => write!(
                f,
                "{functional_id}: a static ability's condition may not count by \
                 `min_power`; a computed power read from inside the layer system \
                 would not terminate"
            ),
            Self::EmblemAbilityIsNotStaticOrTriggered { functional_id } => write!(
                f,
                "{functional_id}: an emblem may carry only static and triggered \
                 abilities (CR 114.1)"
            ),
            Self::TwoVariableTargetGroups { functional_id } => write!(
                f,
                "{functional_id}: one ability or spell may declare at most one \
                 variable-arity (`up_to`) target group"
            ),
            Self::DuplicatePrinting {
                set_code,
                collector_number,
            } => write!(
                f,
                "two printings in {set_code} claim collector number {collector_number}"
            ),
        }
    }
}
