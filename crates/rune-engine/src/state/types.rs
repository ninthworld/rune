//! Game event types, results, permanents, counters, and static effects.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::card::Keyword;
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::player::LossReason;

/// The terminal outcome of a game (CR 104.2a / CR 104.4a), derived on demand from
/// player state — never stored on [`GameState`](crate::GameState), in keeping with the engine's
/// "everything derivable is computed on demand" invariant.
///
/// Produced by [`GameState::result`](crate::GameState::result) once at most one player remains: the sole
/// survivor is the winner (CR 104.2a), or there is no winner when every player has
/// lost simultaneously (a draw, CR 104.4a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameResult {
    /// The single remaining player who won (CR 104.2a), or `None` for a draw where
    /// no player remains (CR 104.4a).
    pub winner: Option<PlayerId>,
    /// Every player who has lost, in seat order (CR 104.3).
    pub losers: Vec<PlayerId>,
    /// Why the game ended: the loss reason of the deciding loser (the sole loser
    /// when there is a winner; the first loser in seat order for a draw).
    pub reason: LossReason,
}

/// A stable, bounded-history entry emitted by the pure engine transition pipeline.
///
/// Entries are values on [`GameState`](crate::GameState), not notifications: projecting them into a
/// view therefore preserves replayability and lets a reconnect reconstruct the
/// same recent history without client-side accumulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameLogEntry {
    /// Monotonically increasing sequence number, starting at one.
    pub sequence: u64,
    /// Structured event payload. The server supplies presentation names and
    /// redacts hidden information while projecting this value.
    pub event: GameEvent,
}

/// A permanent as referenced by a log event, paired with the immutable card
/// identity needed to name it during projection.
///
/// Combatant and death events carry this rather than a bare [`PermanentId`] so a
/// snapshot's history stays stable: the server names the object from the recorded
/// [`card`](Self::card) instead of re-resolving it against the *current*
/// battlefield, which would degrade to "unknown" the instant the permanent leaves
/// (dies, is bounced, …). A [`PermanentId`] is never reused, so the id is still a
/// stable presentation handle a client may highlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoggedPermanent {
    /// Battlefield identity at the moment the event was recorded.
    pub permanent: PermanentId,
    /// The card the permanent represented, for public naming during projection.
    pub card: CardId,
}

/// What a [`GameEvent::DamageDealt`] was dealt to (CR 120.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageTarget {
    /// Damage dealt to a player — life loss (CR 120.3a).
    Player(PlayerId),
    /// Damage marked on a permanent (CR 120.3d), named from its recorded identity.
    Permanent(LoggedPermanent),
}

/// Engine-level facts suitable for a public game log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameEvent {
    /// A player cast a spell represented by this physical card instance.
    SpellCast {
        /// The spell's controller.
        player: PlayerId,
        /// The physical card cast.
        card: CardInstance,
    },
    /// A spell finished resolving (CR 608.3) — it was neither countered nor fizzled.
    SpellResolved {
        /// The spell's controller.
        player: PlayerId,
        /// The physical card that resolved.
        card: CardInstance,
    },
    /// A spell was countered and put into its owner's graveyard (CR 701.5a).
    SpellCountered {
        /// The countered spell's controller.
        player: PlayerId,
        /// The physical card that was countered.
        card: CardInstance,
    },
    /// A spell left the stack without resolving because every one of its targets
    /// became illegal (CR 608.2b, "fizzle").
    SpellFizzled {
        /// The fizzled spell's controller.
        player: PlayerId,
        /// The physical card that fizzled.
        card: CardInstance,
    },
    /// A player declared these battlefield objects as attackers.
    AttackersDeclared {
        /// The attacking player.
        player: PlayerId,
        /// The attacking permanents, each with the identity needed to name it later.
        attackers: Vec<LoggedPermanent>,
    },
    /// A player declared these blocker/attacker pairs.
    BlockersDeclared {
        /// The defending player.
        player: PlayerId,
        /// `(blocker, attacker)` assignments, each carrying naming identity.
        blocks: Vec<(LoggedPermanent, LoggedPermanent)>,
    },
    /// A player took another London mulligan.
    Mulligan {
        /// The player taking a mulligan.
        player: PlayerId,
    },
    /// A player kept their opening hand, ending their mulligan decisions (CR 103.5).
    HandKept {
        /// The player who kept.
        player: PlayerId,
    },
    /// A player's life total changed by this signed amount from a non-damage source
    /// (life gain, or life paid/lost). Damage to a player is a [`Self::DamageDealt`]
    /// event instead, so the log never double-reports combat or burn as life change.
    LifeChanged {
        /// The affected player.
        player: PlayerId,
        /// Signed life-total delta.
        amount: i32,
    },
    /// A source dealt this much damage to a player or permanent (CR 120), including
    /// nonlethal damage a client can report before any death.
    DamageDealt {
        /// What the damage was dealt to.
        target: DamageTarget,
        /// How much damage.
        amount: u32,
    },
    /// A player drew cards; individual hidden cards are deliberately not recorded.
    CardsDrawn {
        /// The player who drew.
        player: PlayerId,
        /// Number of cards drawn.
        count: u32,
    },
    /// A creature left the battlefield for a graveyard (CR 700.4 — a creature
    /// "dies"). Only creatures produce this; an Aura or other permanent moving to a
    /// graveyard is a zone change, not a death.
    PermanentDied {
        /// Battlefield identity before it left, with the identity needed to name it.
        permanent: LoggedPermanent,
    },
    /// The turn structure reached a new step.
    StepChanged {
        /// Current turn number.
        turn: u32,
        /// Active player for that turn.
        active_player: PlayerId,
        /// Newly entered turn step.
        step: crate::phase::Step,
    },
    /// A player left the game under CR 800.4a — they lost while two or more
    /// players remained, so the game continues without them and their objects are
    /// removed. Distinct from [`Self::GameOver`], which fires only once one player
    /// is left; a two-player loss produces `GameOver`, not this.
    PlayerEliminated {
        /// The player who left the game.
        player: PlayerId,
        /// Why they lost (CR 104.3 / 704.5).
        reason: LossReason,
    },
    /// A commander was moved from a graveyard or exile to its owner's command
    /// zone under CR 903.9a, at that owner's choice. Records the movement so a
    /// client can show where the commander went; declining the return records
    /// nothing (the card simply stays where it was).
    CommanderReturnedToCommandZone {
        /// The commander's owner, who made the choice.
        player: PlayerId,
        /// The physical commander card that moved to the command zone.
        card: CardInstance,
    },
    /// The game reached its terminal result.
    GameOver {
        /// Already-derived terminal result.
        result: GameResult,
    },
}

/// A kind of counter that can sit on a [`Permanent`].
///
/// Only the power/toughness counters the layer system folds into computed
/// characteristics today are modeled (ADR 0010 slice 2, CR 613.7c). Other kinds
/// (loyalty, charge, …) are deferred until an effect needs them, at which point
/// a variant is added here. Used as a [`BTreeMap`] key in
/// [`Permanent::counters`], so ordering is derived and replay-stable.
///
/// Deserialized from a bare `snake_case` tag so the effect IR can name a counter
/// kind as card data (e.g. `{"kind": "put_counters", "counter": "plus_one_plus_one"}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CounterKind {
    /// A `+1/+1` counter: adds 1 to power and 1 to toughness (CR 122, CR 613.7c).
    PlusOnePlusOne,
    /// A `-1/-1` counter: subtracts 1 from power and 1 from toughness.
    MinusOneMinusOne,
}

/// A permanent on the shared battlefield.
///
/// Its [`PermanentId`] is minted fresh on battlefield entry and is distinct
/// from the [`CardId`] of the card it represents. It also links the
/// [`CardInstanceId`] of the physical card it originated from, so identity is
/// preserved when the permanent leaves the battlefield for another zone.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Permanent {
    /// Battlefield identity, fresh on entry.
    pub id: PermanentId,
    /// The physical card this permanent originated from. Stable across the zone
    /// change that put it here, unlike [`Self::id`].
    pub instance: CardInstanceId,
    /// The card this permanent represents.
    pub card: CardId,
    /// The player who currently controls it.
    pub controller: PlayerId,
    /// Whether the permanent is tapped.
    pub tapped: bool,
    /// The turn number on which this permanent entered the battlefield under its
    /// current controller (came under their control). Raw stored state, set on
    /// battlefield entry from [`GameState::turn`](crate::GameState::turn); `0` for the empty [`Default`].
    ///
    /// This is the fact "summoning sickness" is derived from (CR 302.6): a
    /// creature has been controlled continuously since its controller's most
    /// recent turn began exactly when it entered on an *earlier* turn than the
    /// current one. The engine cannot derive that from a bare snapshot — it is
    /// history — so, like [`Self::damage`], it is stored rather than computed.
    /// Not a zone-change counter: a permanent re-entering the battlefield gets a
    /// fresh [`PermanentId`] and a fresh `entered_turn`; nothing counts entries.
    pub entered_turn: u32,
    /// Whom this permanent is attacking, i.e. the defending player it was declared
    /// to attack this combat (CR 508.1a — each attacker attacks a chosen defending
    /// player), or `None` if it is not attacking. Raw stored state, set when
    /// attackers are declared and cleared at the end-of-combat step (CR 511.3).
    ///
    /// This is the one field that carries combat's multiplayer generalization: a
    /// two-player game's sole legal defender is the one opponent, but with more
    /// seats each attacker records *which* opponent it attacks, and blocker
    /// eligibility and combat damage follow that assignment (issue #341). `None`
    /// for a permanent not in combat.
    pub attacking: Option<PlayerId>,
    /// The attacker this permanent is blocking, if it was declared as a blocker
    /// this combat (CR 509.1); `None` for a permanent that is not blocking.
    ///
    /// A blocker is assigned to exactly one attacking creature (this field is that
    /// assignment); several blockers may name the same attacker. Raw stored state,
    /// set when blockers are declared and cleared at the end-of-combat step
    /// (CR 511.3).
    pub blocking: Option<PermanentId>,
    /// Damage marked on this permanent this turn (CR 120.3). Raw stored state,
    /// zeroed as a turn-based action during the cleanup step (CR 514.2) and,
    /// once combat lands (issue #118), compared against toughness by the
    /// state-based-actions loop (CR 704.5g). `0` means no marked damage.
    pub damage: u32,
    /// Counters on this permanent, keyed by [`CounterKind`] and mapped to how
    /// many of that kind are present.
    ///
    /// This is **raw stored state, not a derivation** (ADR 0010 §1): nothing
    /// else in [`GameState`](crate::GameState) determines a permanent's counters, so the
    /// "no cached derivations" invariant does not apply to it. Current
    /// power/toughness *is* derived and folds these in on demand via
    /// [`characteristics`](crate::characteristics::characteristics); it is never
    /// stored. A kind absent from the map means zero of that counter; a present
    /// entry is a positive count.
    pub counters: BTreeMap<CounterKind, u32>,
    /// The permanent this one is attached to, if any (CR 303.4 / 701.3) — used
    /// today for an Aura, which enters attached to the object its enchant
    /// ability chose (CR 303.4d) and stays attached until it leaves the
    /// battlefield or its host does.
    ///
    /// **Raw stored state, not a derivation** (ADR 0010 §1): the attachment is a
    /// per-object fact nothing else in [`GameState`](crate::GameState) determines, like
    /// [`Self::counters`]. The Aura's continuous power/toughness contribution to
    /// its host *is* derived from this attachment on demand via
    /// [`characteristics`](crate::characteristics::characteristics) and is never
    /// stored, so it vanishes the instant the Aura leaves (nothing to prune).
    /// `None` for an unattached permanent (every non-Aura today). Only an
    /// on-battlefield [`PermanentId`] is a legal host; a dangling reference (the
    /// host having left) is caught by the CR 704.5m state-based action, which
    /// puts the Aura into its owner's graveyard.
    pub attached_to: Option<PermanentId>,
}

impl Permanent {
    /// How many counters of `kind` are on this permanent, `0` when none are.
    #[must_use]
    pub fn counter_count(&self, kind: CounterKind) -> u32 {
        self.counters.get(&kind).copied().unwrap_or(0)
    }
}

/// A continuous static effect currently in force (ADR 0010 slice 3, §4).
///
/// This is **raw stored input, not a derivation** (ADR 0010 §1): the source
/// ability or permanent puts the effect here and its removal takes it away.
/// Nothing else in [`GameState`](crate::GameState) determines it, so the "no cached derivations"
/// invariant does not apply to it — the same way [`Permanent::counters`] are
/// stored. A permanent's *current* power/toughness folds the applicable effects
/// in on demand via
/// [`characteristics`](crate::characteristics::characteristics) and is never
/// stored; removing an effect from [`GameState::static_effects`](crate::GameState::static_effects) therefore
/// reverts every affected permanent's computed value with nothing to invalidate.
///
/// This slice models only the layer-7c power/toughness modification an anthem or
/// pump performs; other layers slot in as new [`Modification`] variants behind
/// the same read path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticEffect {
    /// Object id of the source that put this effect into force — a permanent's
    /// [`PermanentId`](crate::PermanentId) value today, or a future stack
    /// object's id. It is minted from the monotonic [`GameState::next_object_id`](crate::GameState::next_object_id),
    /// so it is strictly increasing and replay-stable, and it doubles as this
    /// effect's **timestamp**: within a layer, effects apply in ascending
    /// `source` order (CR 613.7, ADR 0010 §4). No wall-clock and no ambient
    /// counter is involved. Because it derives from the source object's id,
    /// removing that source (and this entry with it) reverts the computed value.
    pub source: u64,
    /// Which permanents this effect applies to.
    pub affects: EffectAffects,
    /// The continuous modification this effect performs. The variant fixes the
    /// CR 613 layer; only layer 7c power/toughness modification ships in this
    /// slice.
    pub modification: Modification,
    /// How long this effect lasts before it wears off (CR 611.2).
    ///
    /// A permanent-lifetime anthem is [`Duration::WhileOnBattlefield`]; a pump
    /// spell's "+X/+Y until end of turn" is [`Duration::UntilEndOfTurn`], which
    /// the cleanup step ends (CR 514.2). The duration never affects *which*
    /// permanents an effect touches or its timestamp ordering (CR 613.7) — it
    /// only governs when the effect is removed from [`GameState::static_effects`](crate::GameState::static_effects).
    pub duration: Duration,
}

impl StaticEffect {
    /// This effect's timestamp for intra-layer ordering: its [`source`] object
    /// id (ADR 0010 §4 — the id assigned when the effect was created). Exposed as
    /// a named accessor so ordering code reads by intent rather than by field.
    ///
    /// [`source`]: Self::source
    #[must_use]
    pub fn timestamp(&self) -> u64 {
        self.source
    }
}

/// How long a [`StaticEffect`] lasts before it wears off (CR 611.2).
///
/// A deliberately small closed set for this slice: only the permanent lifetime an
/// anthem-style static ability has and the single "until end of turn" duration a
/// pump spell grants (CR 514.2). Other durations ("until your next turn", "as
/// long as …") are deferred until a card needs them, at which point a variant is
/// added here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Duration {
    /// The effect lasts as long as its source is on the battlefield — the
    /// lifetime of a static ability's continuous effect (an anthem). It is never
    /// removed by the cleanup step; it ends only when its source leaves. The
    /// default, so the pre-existing static-ability effects keep their meaning.
    #[default]
    WhileOnBattlefield,
    /// The effect ends during the cleanup step of the turn it was created in
    /// (CR 514.2): a "+X/+Y until end of turn" pump. Removed simultaneously with
    /// the marked-damage wipe as a single cleanup turn-based action.
    UntilEndOfTurn,
}

/// Selects the permanents a [`StaticEffect`] applies to.
///
/// A deliberately small closed set for this slice: no targeting (that is ADR
/// 0009, a separate decision) and no authored-card selectors yet (those arrive
/// with the cards that create these effects). The one variant models the
/// canonical anthem, "creatures you control".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectAffects {
    /// Every creature controlled by the given player (anthem-style "creatures
    /// you control"). A permanent matches when it is currently a creature and
    /// its controller equals this player.
    CreaturesControlledBy(PlayerId),
    /// The single permanent with this [`PermanentId`] — a pump spell's chosen
    /// target (CR 601.2c). Because a [`PermanentId`] is minted fresh on
    /// battlefield entry and never reused, the effect matches exactly that one
    /// object; once it leaves the battlefield the effect can never apply again
    /// (and is pruned by the state-based-actions loop, so no modifier outlives
    /// its permanent).
    SpecificPermanent(PermanentId),
}

/// The continuous modification a [`StaticEffect`] performs. The variant fixes
/// the CR 613 layer the effect applies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modification {
    /// CR 613 **layer 7c**: add the given signed amounts to power and toughness
    /// (a negative amount subtracts). Applied after counters, in timestamp order
    /// (ADR 0010 §3–§4).
    PowerToughness {
        /// Amount added to power.
        power: i32,
        /// Amount added to toughness.
        toughness: i32,
    },
    /// CR 613 **layer 6** (CR 613.1f): add a keyword ability to the affected
    /// permanent — an aura granting flying, an anthem granting vigilance, or a
    /// pump spell granting trample until end of turn. A granted keyword is
    /// indistinguishable from a printed one everywhere keywords are read
    /// ([`characteristics`](crate::characteristics::characteristics) folds it into
    /// the computed keyword set). Redundant grants are idempotent (CR 702.2c-style:
    /// granting flying twice is simply flying), so this modification never stacks —
    /// it either adds the keyword or leaves an already-present one unchanged. Layer
    /// 6 is timestamp-independent for a pure grant, so unlike layer-7c modifiers
    /// these need not be folded in order.
    GrantKeyword(Keyword),
}

/// One running total of cumulative **combat** damage a commander has dealt a
/// player over the game (CR 903.10a).
///
/// **Raw stored history, not a derivation** (ADR 0010 §1): "how much combat
/// damage has this commander dealt this player *so far*" is a fact a bare
/// snapshot cannot recover — the same reasoning as [`Permanent::damage`] — so it
/// is stored, in [`GameState::commander_damage`](crate::GameState::commander_damage).
///
/// The key is the **commander designation** ([`Self::commander`], the owning
/// player) and the [`damaged`](Self::damaged) player, **never** a
/// [`PermanentId`]. A commander is minted a fresh `PermanentId` on every
/// battlefield entry, so keying the tally to a permanent would silently reset it
/// each time the commander changed zones and re-entered; keying it to the
/// designation (which one player has at most one of today, so its owner's
/// [`PlayerId`] identifies it) makes the total survive those zone changes and
/// recasts, exactly as CR 903.10a's "any one commander" requires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommanderDamage {
    /// The commander that dealt the damage, identified by its owning player — the
    /// stable designation key (one commander per player today). Survives the
    /// commander's fresh [`PermanentId`] on every battlefield re-entry.
    pub commander: PlayerId,
    /// The player the commander has dealt combat damage to.
    pub damaged: PlayerId,
    /// Cumulative combat damage this commander has dealt this player this game.
    pub amount: u32,
}
