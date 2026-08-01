//! Catalog schema validation, shared verbatim by `build.rs` and the loader.
//!
//! This module is compiled **twice**: once as part of the engine (`mod catalog`), and
//! once by `crates/sage-engine/build.rs`, which pulls this exact file in with
//! `#[path = "src/catalog.rs"] mod catalog;`. That is why it depends on nothing but
//! `std` and `serde_json`, and never names a `crate::` path: `build.rs` is compiled
//! *before* the engine exists, so it cannot borrow the engine's types.
//!
//! Compiling one file in both places is what makes ADR 0008 §5's promise —
//! "the same validators run under `#[cfg(test)]`" — literally true rather than
//! aspirational. A rule stated here is enforced when the catalog is assembled
//! (`build.rs`, so a bad card file fails `cargo build`), again when a snapshot is
//! loaded ([`crate::CardDatabase`]), and again by this module's own unit tests, with
//! no second copy to drift out of step.
//!
//! Everything here works on [`serde_json::Value`] rather than the typed
//! [`CardData`](crate::CardData) precisely because `build.rs` cannot see that type.
//! The division of labor is deliberate:
//!
//! - **Here**: rules about a definition's *shape* that hold before the IR is known —
//!   the schema version, the authored identity, the type/P&T and Aura invariants.
//! - **In the type system**: rules serde already makes unrepresentable. Every
//!   targeting [`Effect`](crate::Effect) variant declares `target: TargetSpec` as a
//!   required field, as does [`AuraGrant::enchant`](crate::AuraGrant::enchant), so
//!   "an effect that needs a target spec but has none" cannot be written down — it is
//!   a parse error, not a validation failure. No check here re-states it.
//! - **In the loader**: the one rule that is impossible to check here — whether a
//!   definition's `scripted` flag agrees with `crates/sage-engine/src/scripted.rs`.
//!   That answer lives in compiled Rust, which does not exist yet when `build.rs`
//!   runs, so [`CardDatabase::from_json`](crate::CardDatabase::from_json) owns it (in
//!   both directions — ADR 0008 §5).

use std::fmt;

/// The functional-definition schema version this engine understands (ADR 0008 §2).
///
/// Re-exported as `sage_engine::SCHEMA_VERSION`. A definition declaring any other
/// version is a hard error ([`Violation::UnsupportedSchemaVersion`]), never a silent
/// skip: a breaking change to the schema's shape bumps this, so the whole catalog is
/// migrated under one forcing function instead of half-loading.
pub const SCHEMA_VERSION: u32 = 1;

/// The subtype that makes a card an Aura (CR 303.4), and therefore the only kind of
/// card an `aura` grant may appear on.
const AURA_SUBTYPE: &str = "Aura";

/// The card type that requires printed power and toughness.
const CREATURE_TYPE: &str = "creature";

/// The card type that requires a printed starting loyalty (CR 306.5b).
const PLANESWALKER_TYPE: &str = "planeswalker";

/// The card types a permanent may have (CR 110.1) — and therefore the only types a
/// **token** may have, a token existing nowhere but the battlefield (CR 111.7).
const PERMANENT_TYPES: [&str; 6] = [
    "land",
    "creature",
    "artifact",
    "enchantment",
    "planeswalker",
    "battle",
];

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
    /// An optional effect (`{"kind":"may"}`) wraps an effect that chooses a target.
    ///
    /// One effect declares at most one target slot, so a wrapper cannot declare the
    /// slots of what it wraps: the target would never be chosen at announcement
    /// (CR 601.2c) and the nested effect would silently do nothing on acceptance. That
    /// is a card that looks authored and is not, which is worth failing the build over.
    TargetInsideOptional {
        /// The definition at fault.
        functional_id: String,
    },
    /// A conditional effect (`{"kind":"conditional"}`) has a branch that chooses a
    /// target.
    ///
    /// The same rule — and the same reason — as [`Self::TargetInsideOptional`]: one
    /// effect declares at most one target group, so a wrapper cannot declare the groups
    /// of what it wraps. A branch that targeted would have its slot filled by nobody at
    /// announcement (CR 601.2c) and silently do nothing.
    TargetInsideConditional {
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
            Self::TargetInsideOptional { functional_id } => write!(
                f,
                "{functional_id} has a `may` effect wrapping an effect that targets; \
                 an optional effect's contents may not choose a target"
            ),
            Self::TargetInsideConditional { functional_id } => write!(
                f,
                "{functional_id}: a conditional effect's branch may not choose a target"
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

/// Whether `slug` is a well-formed [`FunctionalId`](crate::FunctionalId): a non-empty
/// lowercase `snake_case` identifier starting with a letter, with no doubled or
/// trailing underscore (e.g. `onakke_ogre`).
///
/// The single definition of the rule. `FunctionalId::try_from` enforces it on the
/// typed side and `build.rs` enforces it on catalog files, both through this function,
/// so an identity cannot be legal in one place and illegal in the other.
#[must_use]
pub(crate) fn is_well_formed_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.starts_with(|c: char| c.is_ascii_lowercase())
        && !slug.ends_with('_')
        && !slug.contains("__")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Validate one functional definition, returning its `functional_id`.
///
/// `file_stem` is the name of the file the definition came from, without its `.json`
/// extension — `Some` when validating the sharded catalog (where the file name *is*
/// the identity, ADR 0008 §4), and `None` when validating a snapshot that has no file
/// behind it, such as a test fixture or an in-memory array.
///
/// # Errors
/// Returns the first [`Violation`] found. Checks run identity-first, so every later
/// message can name the card it is complaining about.
pub(crate) fn validate_definition(
    file_stem: Option<&str>,
    value: &serde_json::Value,
) -> Result<String, Violation> {
    let object = value.as_object().ok_or(Violation::NotAnObject)?;

    // Identity first: everything below reports against it.
    let functional_id = object
        .get("functional_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Violation::MalformedField {
            functional_id: file_stem.unwrap_or("<unknown>").to_string(),
            field: "functional_id",
        })?
        .to_string();

    if !is_well_formed_slug(&functional_id) {
        return Err(Violation::MalformedFunctionalId {
            slug: functional_id,
        });
    }
    if let Some(stem) = file_stem {
        if stem != functional_id {
            return Err(Violation::FileNameMismatch {
                functional_id,
                file_stem: stem.to_string(),
            });
        }
    }

    let version = object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| Violation::MalformedField {
            functional_id: functional_id.clone(),
            field: "schema_version",
        })?;
    if version != u64::from(SCHEMA_VERSION) {
        return Err(Violation::UnsupportedSchemaVersion {
            functional_id,
            found: version,
        });
    }

    let types = object
        .get("types")
        .and_then(serde_json::Value::as_array)
        .filter(|types| !types.is_empty())
        .ok_or_else(|| Violation::MalformedField {
            functional_id: functional_id.clone(),
            field: "types",
        })?;

    // A Creature carries printed power and toughness; nothing else may (ADR 0008 §5).
    // Checked as a pair: half a P/T is as wrong as none at all on a creature.
    let is_creature = types.iter().any(|t| t.as_str() == Some(CREATURE_TYPE));
    let has_power = object.contains_key("power");
    let has_toughness = object.contains_key("toughness");
    if is_creature != (has_power && has_toughness) || has_power != has_toughness {
        return Err(Violation::PowerToughnessMismatch {
            functional_id,
            creature: is_creature,
        });
    }

    // A Planeswalker carries a printed starting loyalty and nothing else does
    // (CR 306.5b) — the same both-directions pairing the P/T check above makes, and for
    // the same reason: a planeswalker with none would die to CR 704.5i on arrival.
    let is_planeswalker = types.iter().any(|t| t.as_str() == Some(PLANESWALKER_TYPE));
    if is_planeswalker != object.contains_key("loyalty") {
        return Err(Violation::LoyaltyMismatch {
            functional_id,
            planeswalker: is_planeswalker,
        });
    }

    // A printed combat restriction only ever restricts attacking or blocking, so it
    // belongs only on a creature. An Aura *grants* restrictions to its host through
    // `aura.restrictions` instead, which is why this looks only at the printed list.
    if object.contains_key("restrictions") && !is_creature {
        return Err(Violation::RestrictionsOnNonCreature { functional_id });
    }

    // An optional effect may not wrap a targeting one (see
    // [`Violation::TargetInsideOptional`]). Checked over every effect list a definition
    // carries, at any nesting depth, so a `may` inside a `may` is covered too.
    if authored_effects(object).any(optional_wraps_a_target) {
        return Err(Violation::TargetInsideOptional { functional_id });
    }

    // A conditional's branches are wrapped effects and follow the optional effect's rule
    // for the same reason: a wrapper cannot honestly declare the target groups of what it
    // wraps.
    if every_effect(object)
        .into_iter()
        .any(conditional_wraps_a_target)
    {
        return Err(Violation::TargetInsideConditional { functional_id });
    }

    // CR 114.1: an emblem has no characteristics but its abilities, and only the two
    // kinds that need neither an activation nor an entry event can function on one.
    if every_effect(object).into_iter().any(emblem_ability_is_bad) {
        return Err(Violation::EmblemAbilityIsNotStaticOrTriggered { functional_id });
    }

    // At most one "up to N" target group per ability or spell, so the flat stored target
    // list pairs back onto effects unambiguously.
    if effect_lists(object)
        .into_iter()
        .any(|effects| variable_target_groups(&effects) > 1)
    {
        return Err(Violation::TwoVariableTargetGroups { functional_id });
    }

    // Every token a definition creates must be an object that could exist: a permanent
    // (CR 110.1/111.7), with power and toughness exactly when it is a creature. Walked
    // to any depth, so a `create_token` nested inside a `may` is checked too.
    for effect in every_effect(object) {
        if effect.get("kind").and_then(serde_json::Value::as_str) != Some("create_token") {
            continue;
        }
        validate_token(&functional_id, effect.get("token"))?;
    }

    // An `aura` grant is the Aura ability (CR 303.4), so it belongs only on an Aura.
    if object.contains_key("aura") {
        let is_aura = object
            .get("subtypes")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|subtypes| subtypes.iter().any(|s| s.as_str() == Some(AURA_SUBTYPE)));
        if !is_aura {
            return Err(Violation::AuraOnNonAura { functional_id });
        }
    }

    Ok(functional_id)
}

/// Validate the `token` block of a `create_token` effect, authored by `functional_id`.
///
/// The token analogue of the type and power/toughness rules [`validate_definition`]
/// applies to a card, and deliberately the same two: an object that is not a permanent
/// could not be on the battlefield, and a creature without printed power and toughness
/// is not a creature anyone can play with. Everything else a token may not have — a
/// `functional_id`, a mana cost, a `scripted` flag — is unrepresentable in
/// [`TokenData`](crate::TokenData) rather than checked here, so it is a parse error
/// instead of a validation one.
fn validate_token(functional_id: &str, token: Option<&serde_json::Value>) -> Result<(), Violation> {
    // A missing or malformed `token` is a parse error on the typed side; there is
    // nothing to validate here.
    let Some(token) = token.and_then(serde_json::Value::as_object) else {
        return Ok(());
    };
    let types = token
        .get("types")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let is_permanent = !types.is_empty()
        && types.iter().all(|t| {
            t.as_str()
                .is_some_and(|name| PERMANENT_TYPES.contains(&name))
        });
    if !is_permanent {
        return Err(Violation::TokenIsNotAPermanent {
            functional_id: functional_id.to_string(),
        });
    }
    let is_creature = types.iter().any(|t| t.as_str() == Some(CREATURE_TYPE));
    let has_power = token.contains_key("power");
    let has_toughness = token.contains_key("toughness");
    if is_creature != (has_power && has_toughness) || has_power != has_toughness {
        return Err(Violation::TokenPowerToughnessMismatch {
            functional_id: functional_id.to_string(),
            creature: is_creature,
        });
    }
    Ok(())
}

/// Every effect a definition authors, at any nesting depth — the top-level lists
/// [`authored_effects`] yields, plus everything nested inside them (the contents of a
/// `may`).
///
/// Used by rules that are about an effect wherever it appears, rather than about the
/// shape of the list it sits in.
fn every_effect(object: &serde_json::Map<String, serde_json::Value>) -> Vec<&serde_json::Value> {
    fn walk<'a>(effect: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        out.push(effect);
        for nested in nested_effects(effect) {
            walk(nested, out);
        }
    }
    let mut out = Vec::new();
    for effect in authored_effects(object) {
        walk(effect, &mut out);
    }
    out
}

/// The effects nested inside `effect`, whatever key they hang off: a `may`'s `effects`,
/// a `conditional`'s `then` and `otherwise`, and the effect lists of the abilities a
/// `create_emblem` hands out.
///
/// One function so a rule stated about "every effect a definition authors" cannot be
/// true of one wrapper and quietly false of the next one added.
fn nested_effects(effect: &serde_json::Value) -> Vec<&serde_json::Value> {
    let mut out = Vec::new();
    for key in ["effects", "then", "otherwise"] {
        out.extend(
            effect
                .get(key)
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
    }
    for ability in effect
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        out.extend(
            ability
                .get("effects")
                .and_then(serde_json::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
    }
    out
}

/// Every **list** of effects a definition authors as one announcement's worth: each
/// ability's `effects`, the card's `spell_effects`, and each ability an emblem is created
/// with.
///
/// Distinct from [`every_effect`], which flattens: the variable-arity rule is about what
/// one *object on the stack* declares together, so it has to see the lists rather than
/// the effects.
fn effect_lists(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<Vec<&serde_json::Value>> {
    let mut lists: Vec<Vec<&serde_json::Value>> = Vec::new();
    for ability in object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        if let Some(effects) = ability.get("effects").and_then(serde_json::Value::as_array) {
            lists.push(effects.iter().collect());
        }
    }
    if let Some(effects) = object
        .get("spell_effects")
        .and_then(serde_json::Value::as_array)
    {
        lists.push(effects.iter().collect());
    }
    for effect in every_effect(object) {
        for ability in effect
            .get("abilities")
            .and_then(serde_json::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            if let Some(effects) = ability.get("effects").and_then(serde_json::Value::as_array) {
                lists.push(effects.iter().collect());
            }
        }
    }
    lists
}

/// How many of `effects` declare an `"up_to"` target count — the variable-arity groups
/// one announcement would have to split its flat target list between.
fn variable_target_groups(effects: &[&serde_json::Value]) -> usize {
    effects
        .iter()
        .filter(|effect| {
            effect
                .get("targets")
                .and_then(serde_json::Value::as_object)
                .is_some_and(|count| count.contains_key("up_to"))
        })
        .count()
}

/// Whether `effect` is a `conditional` whose branches would choose a target.
fn conditional_wraps_a_target(effect: &serde_json::Value) -> bool {
    if effect.get("kind").and_then(serde_json::Value::as_str) != Some("conditional") {
        return false;
    }
    ["then", "otherwise"].into_iter().any(|key| {
        effect
            .get(key)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|branch| branch.iter().any(effect_chooses_a_target))
    })
}

/// Whether `effect` is a `create_emblem` handing out an ability an emblem cannot carry
/// (CR 114.1) — anything but `static` or `triggered`.
fn emblem_ability_is_bad(effect: &serde_json::Value) -> bool {
    if effect.get("kind").and_then(serde_json::Value::as_str) != Some("create_emblem") {
        return false;
    }
    effect
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .any(|ability| {
            !matches!(
                ability.get("type").and_then(serde_json::Value::as_str),
                Some("static" | "triggered")
            )
        })
}

/// Every effect a definition authors at the top level of an ability or of its spell
/// effects, in file order.
///
/// Shallow on purpose: the nested contents of an effect are the business of whatever
/// walks *that* effect ([`optional_wraps_a_target`] recurses into its own).
fn authored_effects(
    object: &serde_json::Map<String, serde_json::Value>,
) -> impl Iterator<Item = &serde_json::Value> {
    let abilities = object
        .get("abilities")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|ability| ability.get("effects"))
        .filter_map(serde_json::Value::as_array)
        .flatten();
    let spell = object
        .get("spell_effects")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter();
    abilities.chain(spell)
}

/// Whether `effect` is a `may` (or contains one) whose optional contents would choose a
/// target.
///
/// Recursive because a `may` may wrap a `may`, and the rule is about the whole subtree
/// under the outermost one: anything targeting *anywhere* inside an optional effect is
/// a slot no announcement ever fills.
fn optional_wraps_a_target(effect: &serde_json::Value) -> bool {
    let Some(kind) = effect.get("kind").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let nested = nested_effects(effect);
    if kind == "may" {
        return nested.iter().any(|e| effect_chooses_a_target(e));
    }
    nested.iter().any(|e| optional_wraps_a_target(e))
}

/// Whether `effect`, or anything nested inside it, chooses a target (CR 115.1).
///
/// Two authored spellings say "target", and both count: a `target` spec on the effect
/// itself, and a `player_ref` naming a targeted seat. Kept here rather than in the typed
/// IR because `build.rs` validates JSON before the IR exists (ADR 0008 §5).
fn effect_chooses_a_target(effect: &serde_json::Value) -> bool {
    if effect.get("target").is_some() {
        return true;
    }
    if matches!(
        effect.get("player_ref").and_then(serde_json::Value::as_str),
        Some("target_player" | "target_opponent")
    ) {
        return true;
    }
    nested_effects(effect)
        .into_iter()
        .any(effect_chooses_a_target)
}

/// Reject two printings in one set claiming the same collector number.
///
/// A set's printings are keyed by `(set_code, collector_number)`, so a repeat would
/// silently shadow the earlier record rather than fail. Shared by `build.rs` and
/// [`PrintingDatabase`](crate::PrintingDatabase) so both reject it identically.
///
/// # Errors
/// Returns [`Violation::DuplicatePrinting`] naming the first repeated number.
pub(crate) fn check_printings<'a>(
    set_code: &str,
    collector_numbers: impl IntoIterator<Item = &'a str>,
) -> Result<(), Violation> {
    let mut seen = std::collections::HashSet::new();
    for collector_number in collector_numbers {
        if !seen.insert(collector_number) {
            return Err(Violation::DuplicatePrinting {
                set_code: set_code.to_string(),
                collector_number: collector_number.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// A minimal valid definition, as parsed JSON, that each test then breaks in one way.
    fn definition(extra: &str) -> serde_json::Value {
        let json = format!(
            r#"{{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                 "types": ["creature"], "mana_cost": "{{G}}", "power": 1, "toughness": 1{extra}}}"#
        );
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn a_well_formed_definition_validates_and_yields_its_identity() {
        let id = validate_definition(Some("test_card"), &definition("")).unwrap();
        assert_eq!(id, "test_card");
    }

    #[test]
    fn a_definition_must_be_one_object_not_the_old_monolithic_array() {
        let array = serde_json::from_str(r#"[{"functional_id": "test_card"}]"#).unwrap();
        assert_eq!(
            validate_definition(Some("test_card"), &array).unwrap_err(),
            Violation::NotAnObject
        );
    }

    #[test]
    fn an_unrecognized_schema_version_is_rejected() {
        let mut card = definition("");
        card["schema_version"] = serde_json::json!(SCHEMA_VERSION + 1);
        assert_eq!(
            validate_definition(Some("test_card"), &card).unwrap_err(),
            Violation::UnsupportedSchemaVersion {
                functional_id: "test_card".to_string(),
                found: u64::from(SCHEMA_VERSION) + 1,
            }
        );
    }

    #[test]
    fn a_functional_id_that_does_not_match_its_file_name_is_rejected() {
        assert_eq!(
            validate_definition(Some("some_other_file"), &definition("")).unwrap_err(),
            Violation::FileNameMismatch {
                functional_id: "test_card".to_string(),
                file_stem: "some_other_file".to_string(),
            }
        );
    }

    #[test]
    fn a_snapshot_with_no_file_behind_it_skips_the_file_name_check() {
        assert!(validate_definition(None, &definition("")).is_ok());
    }

    #[test]
    fn an_ill_formed_slug_is_rejected() {
        for slug in [
            "Thornback_Boar",
            "thornback boar",
            "9lives",
            "trailing_",
            "double__bar",
        ] {
            let mut card = definition("");
            card["functional_id"] = serde_json::json!(slug);
            assert_eq!(
                validate_definition(None, &card).unwrap_err(),
                Violation::MalformedFunctionalId {
                    slug: slug.to_string()
                },
                "expected `{slug}` to be rejected"
            );
        }
    }

    #[test]
    fn well_formed_slugs_are_accepted() {
        for slug in ["forest", "onakke_ogre", "druid_of_the_cowl", "b2_bomber"] {
            assert!(
                is_well_formed_slug(slug),
                "expected `{slug}` to be accepted"
            );
        }
    }

    #[test]
    fn a_creature_without_power_and_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{G}"}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::PowerToughnessMismatch {
                functional_id: "test_card".to_string(),
                creature: true,
            }
        );
    }

    #[test]
    fn issue_608_loyalty_is_required_on_a_planeswalker_and_forbidden_elsewhere() {
        // CR 306.5b, and both directions of it. A planeswalker with no starting loyalty
        // would enter with no counters and be put straight into its owner's graveyard
        // by CR 704.5i; a loyalty on anything else is a number nothing would read.
        let missing = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                          "supertypes": ["legendary"], "types": ["planeswalker"],
                          "mana_cost": "{2}{W}{W}"}"#;
        assert_eq!(
            validate_definition(None, &serde_json::from_str(missing).unwrap()).unwrap_err(),
            Violation::LoyaltyMismatch {
                functional_id: "test_card".to_string(),
                planeswalker: true,
            }
        );

        let spurious = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                           "types": ["creature"], "mana_cost": "{G}",
                           "power": 1, "toughness": 1, "loyalty": 3}"#;
        assert_eq!(
            validate_definition(None, &serde_json::from_str(spurious).unwrap()).unwrap_err(),
            Violation::LoyaltyMismatch {
                functional_id: "test_card".to_string(),
                planeswalker: false,
            }
        );

        // And the well-formed pair passes: a planeswalker with loyalty and no P/T.
        let good = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "supertypes": ["legendary"], "types": ["planeswalker"],
                       "mana_cost": "{2}{W}{W}", "loyalty": 4}"#;
        assert_eq!(
            validate_definition(None, &serde_json::from_str(good).unwrap()).unwrap(),
            "test_card"
        );
    }

    #[test]
    fn issue_605_a_token_that_could_not_be_a_permanent_is_rejected() {
        // A token exists only on the battlefield (CR 111.7), so a token that is not a
        // permanent could never exist — the card would author an object with nowhere
        // to go.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["sorcery"], "mana_cost": "{G}",
                       "spell_effects": [{"kind": "create_token",
                         "token": {"name": "Idea", "types": ["instant"]}}]}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::TokenIsNotAPermanent {
                functional_id: "test_card".to_string(),
            }
        );

        // Naming no types at all is the same failure.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["sorcery"], "mana_cost": "{G}",
                       "spell_effects": [{"kind": "create_token",
                         "token": {"name": "Nothing", "types": []}}]}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_err());
    }

    #[test]
    fn issue_605_a_token_needs_power_and_toughness_exactly_when_it_is_a_creature() {
        // The token counterpart of the card rule, and wrong for the same reason.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["sorcery"], "mana_cost": "{G}",
                       "spell_effects": [{"kind": "create_token",
                         "token": {"name": "Goblin", "types": ["creature"]}}]}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::TokenPowerToughnessMismatch {
                functional_id: "test_card".to_string(),
                creature: true,
            }
        );

        // A noncreature token carrying power/toughness is the other direction.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["sorcery"], "mana_cost": "{G}",
                       "spell_effects": [{"kind": "create_token",
                         "token": {"name": "Treasure", "types": ["artifact"],
                                   "power": 1, "toughness": 1}}]}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_err());
    }

    #[test]
    fn issue_605_a_token_nested_inside_an_optional_effect_is_still_validated() {
        // The walk is to any depth: a `create_token` inside a `may` is checked too,
        // so nesting is not a way around the rule.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["sorcery"], "mana_cost": "{G}",
                       "spell_effects": [{"kind": "may", "effects": [
                         {"kind": "create_token",
                          "token": {"name": "Goblin", "types": ["creature"]}}]}]}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(matches!(
            validate_definition(None, &card).unwrap_err(),
            Violation::TokenPowerToughnessMismatch { .. }
        ));
    }

    #[test]
    fn a_creature_with_only_half_a_power_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{G}", "power": 2}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_err());
    }

    #[test]
    fn a_non_creature_carrying_power_and_toughness_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["instant"], "mana_cost": "{R}", "power": 1, "toughness": 1}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::PowerToughnessMismatch {
                functional_id: "test_card".to_string(),
                creature: false,
            }
        );
    }

    #[test]
    fn an_aura_grant_on_a_card_that_is_not_an_aura_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["enchantment"], "subtypes": ["Shrine"], "mana_cost": "{G}",
                       "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::AuraOnNonAura {
                functional_id: "test_card".to_string()
            }
        );
    }

    #[test]
    fn issue_606_printed_restrictions_on_a_non_creature_are_rejected() {
        // A combat restriction restricts attacking or blocking, so on a non-creature it
        // could only ever be inert. An Aura imposes restrictions through `aura` instead.
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["enchantment"], "mana_cost": "{1}",
                       "restrictions": ["cant_block"]}"#;
        assert_eq!(
            validate_definition(None, &serde_json::from_str(json).unwrap()).unwrap_err(),
            Violation::RestrictionsOnNonCreature {
                functional_id: "test_card".to_string()
            }
        );
    }

    #[test]
    fn issue_610_a_may_wrapping_a_targeting_effect_is_rejected() {
        // A wrapper cannot declare the target slot of what it wraps, so the target
        // would never be chosen and the effect would silently do nothing. Both authored
        // spellings of "target" are caught, in an ability and in a spell effect alike,
        // and nesting does not hide either.
        let spec = r#", "abilities": [{"type": "activated", "cost": [],
            "effects": [{"kind": "may", "effects": [{"kind": "tap", "target": "any_creature"}]}]}]"#;
        assert_eq!(
            validate_definition(None, &definition(spec)),
            Err(Violation::TargetInsideOptional {
                functional_id: "test_card".to_string(),
            }),
        );

        let player_ref = r#", "abilities": [{"type": "activated", "cost": [],
            "effects": [{"kind": "may", "cost": "{1}",
                         "effects": [{"kind": "may",
                                      "effects": [{"kind": "mill", "player_ref": "target_player",
                                                   "count": 2}]}]}]}]"#;
        assert!(validate_definition(None, &definition(player_ref)).is_err());

        let nested_in_a_spell = r#", "spell_effects": [{"kind": "may",
            "effects": [{"kind": "deal_damage", "target": "any_target", "amount": 2}]}]"#;
        assert!(validate_definition(None, &definition(nested_in_a_spell)).is_err());
    }

    #[test]
    fn issue_610_targets_outside_an_optional_effect_are_untouched() {
        // The rule is about what a `may` *wraps*, not about the card: a targeting
        // effect beside an optional one is ordinary and stays authorable, and so does a
        // non-targeting effect inside the optional one.
        let json = r#", "spell_effects": [{"kind": "deal_damage", "target": "any_target",
                                            "amount": 2},
                                          {"kind": "may", "cost": "{1}",
                                           "effects": [{"kind": "draw_card", "count": 1}]}]"#;
        assert!(validate_definition(None, &definition(json)).is_ok());
    }

    #[test]
    fn issue_606_printed_restrictions_on_a_creature_are_accepted() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["creature"], "mana_cost": "{1}", "power": 1, "toughness": 1,
                       "restrictions": ["cant_be_blocked_by_more_than_one"]}"#;
        assert!(validate_definition(None, &serde_json::from_str(json).unwrap()).is_ok());
    }

    #[test]
    fn an_aura_grant_on_an_aura_is_accepted() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": ["enchantment"], "subtypes": ["Aura"], "mana_cost": "{G}",
                       "aura": {"enchant": "any_creature", "power": 1, "toughness": 1}}"#;
        let card = serde_json::from_str(json).unwrap();
        assert!(validate_definition(None, &card).is_ok());
    }

    #[test]
    fn a_definition_with_no_types_is_rejected() {
        let json = r#"{"schema_version": 1, "functional_id": "test_card", "name": "Test Card",
                       "types": [], "mana_cost": "{G}"}"#;
        let card = serde_json::from_str(json).unwrap();
        assert_eq!(
            validate_definition(None, &card).unwrap_err(),
            Violation::MalformedField {
                functional_id: "test_card".to_string(),
                field: "types",
            }
        );
    }

    #[test]
    fn duplicate_collector_numbers_in_one_set_are_rejected() {
        assert!(check_printings("FIX", ["1", "2", "3"]).is_ok());
        assert_eq!(
            check_printings("FIX", ["1", "2", "1"]).unwrap_err(),
            Violation::DuplicatePrinting {
                set_code: "FIX".to_string(),
                collector_number: "1".to_string(),
            }
        );
    }
}
