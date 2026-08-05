//! Game event types, results, permanents, counters, and static effects.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::card::{CombatRestriction, Keyword};
use crate::id::{CardId, CardInstance, CardInstanceId, PermanentId, PlayerId};
use crate::player::LossReason;
use crate::token::Printed;

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

/// A permanent as referenced by a log event, paired with the immutable identity
/// needed to name it during projection.
///
/// Combatant and death events carry this rather than a bare [`PermanentId`] so a
/// snapshot's history stays stable: the server names the object from the recorded
/// [`identity`](Self::identity) instead of re-resolving it against the *current*
/// battlefield, which would degrade to "unknown" the instant the permanent leaves
/// (dies, is bounced, …). A [`PermanentId`] is never reused, so the id is still a
/// stable presentation handle a client may highlight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedPermanent {
    /// Battlefield identity at the moment the event was recorded.
    pub permanent: PermanentId,
    /// What the permanent was, for public naming during projection.
    pub identity: LoggedIdentity,
}

impl LoggedPermanent {
    /// Record `perm` as a log entity, capturing whatever names it (CR 111: a token
    /// has no card, so its own name is what the log has to work with).
    #[must_use]
    pub(crate) fn of(perm: &Permanent) -> Self {
        Self {
            permanent: perm.id,
            identity: match &perm.printed {
                crate::token::Printed::Card(card) => LoggedIdentity::Card(*card),
                crate::token::Printed::Token(token) => LoggedIdentity::Token(token.name.clone()),
            },
        }
    }
}

/// How a [`LoggedPermanent`] is named when the log is projected.
///
/// A card is recorded as its [`CardId`] and named from the database, so the log never
/// carries presentation text the server owns. A token has no card to look up (CR 111),
/// and it has ceased to exist by the time anyone reads the entry — so its *name*, the
/// only characteristic that survives it, is recorded instead. That is the whole
/// difference, and it is why this is an enum rather than an `Option<CardId>` that
/// would leave every token in the log called "unknown".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoggedIdentity {
    /// The card the permanent represented; the server resolves the display name.
    Card(CardId),
    /// The name of the token the permanent was (CR 111.3 — a token's characteristics
    /// come from the effect that made it, and nothing outlives it to look up).
    Token(String),
}

/// What a [`GameEvent::DamageDealt`] was dealt to (CR 120.3).
///
/// Clone but not `Copy`: a damaged permanent is named by its recorded
/// [`LoggedPermanent`], which for a token carries the token's own name (CR 111) —
/// the one characteristic that outlives it.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// A player put cards from the top of their library into their graveyard
    /// (CR 701.13, "mill"). Distinct from [`Self::CardsDrawn`] because milling is not
    /// drawing: it never trips the CR 704.5c decking loss, and a player asked to mill
    /// past an empty library simply mills fewer. The count is what actually moved.
    /// Card identities are recorded no more than a draw's are — a graveyard is public,
    /// so the cards themselves are already visible in the view.
    CardsMilled {
        /// The player who milled.
        player: PlayerId,
        /// Number of cards that actually moved to the graveyard.
        count: u32,
        /// **Which** cards moved, in the order they were milled.
        ///
        /// Recorded where a draw's and a discard's are not, and the asymmetry is the
        /// point: a milled card lands in a public graveyard, so naming it here leaks
        /// nothing that is not already visible — while a `if at least one Zombie card
        /// was milled this way` condition ([`Condition::MilledThisWay`](crate::Condition))
        /// cannot be answered from the graveyard, which cannot tell a card milled this
        /// way from one that was already there. The projection to the wire still carries
        /// only the count.
        cards: Vec<CardInstance>,
    },
    /// A player discarded cards from their hand (CR 701.8). Card identities are
    /// deliberately absent for the same reason [`Self::CardsDrawn`]'s are — a hand is
    /// hidden, and the cards become visible on their own once they are in the public
    /// graveyard. The count is what actually moved, so a player asked to discard more
    /// than they hold logs what they had.
    CardsDiscarded {
        /// The player who discarded.
        player: PlayerId,
        /// Number of cards that actually moved to the graveyard.
        count: u32,
    },
    /// A player searched their library and shuffled it (CR 701.19). Neither what they
    /// looked at nor what they found is recorded: a library is hidden from every other
    /// seat, and naming the found card here would leak it to all of them before it
    /// arrives anywhere public. That the search *happened* is public information —
    /// everyone at the table sees the deck picked up — and the shuffle is why a failed
    /// search is not a free look.
    LibrarySearched {
        /// The player who searched.
        player: PlayerId,
    },
    /// A player took an optional effect they were offered (`you may …`, CR 608.2),
    /// paying its cost if it had one.
    ///
    /// Recorded because the alternative is a silent one: an optional effect that
    /// happens looks exactly like a mandatory one, and an optional effect that does not
    /// looks exactly like a bug. What was offered is not recorded — the ability's text
    /// is public and says so — only that the question was answered, and how.
    OptionalApplied {
        /// The player who accepted (the offering ability's controller).
        player: PlayerId,
    },
    /// A player declined an optional effect (`you may …`, CR 608.2), or was never
    /// asked because its cost was beyond anything they could pay.
    ///
    /// The two cases share an event on purpose: from every other seat they are the same
    /// public fact — the effect was offered and did not happen — and distinguishing them
    /// would report on a pool the rest of the table cannot see.
    OptionalDeclined {
        /// The player who declined (the offering ability's controller).
        player: PlayerId,
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
/// Two power/toughness counters the layer system folds into computed characteristics
/// (ADR 0005 slice 2, CR 613.7c), plus **loyalty** (CR 306.5b), which folds into
/// nothing: it is read as a quantity in its own right, by the loyalty-ability cost, by
/// damage (CR 120.3c), and by the CR 704.5i state-based action. Other kinds (charge,
/// …) are deferred until an effect needs them, at which point a variant is added here.
/// Used as a [`BTreeMap`] key in [`Permanent::counters`], so ordering is derived and
/// replay-stable.
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
    /// A **loyalty** counter (CR 306.5b): the resource a planeswalker enters with
    /// (equal to its printed loyalty), spends and gains through its loyalty abilities
    /// (CR 606.1), loses to damage (CR 120.3c), and dies at zero of (CR 704.5i).
    ///
    /// Deliberately the *same* mechanism as the P/T counters rather than a field on
    /// [`Permanent`]: every rule that touches loyalty is a rule about counters, so a
    /// dedicated field would need each of them restated. It folds into no
    /// characteristic — a planeswalker has no power or toughness — which is why the
    /// layer-7c delta ignores it.
    Loyalty,
    /// A **charge** counter: the generic "this artifact holds N uses" counter, put on
    /// by one activated ability and spent by another ([`Cost::RemoveCounters`]).
    ///
    /// It folds into no characteristic and no state-based action — it is a quantity the
    /// card's own abilities read, and nothing else in the rules knows it exists. That is
    /// what makes it, and the three below, cheap: a counter kind with no rules attached
    /// is a name and a count.
    Charge,
    /// A **gold** counter: charge by another name, on a card that says gold.
    ///
    /// Kept distinct from [`Self::Charge`] rather than aliased because two cards on one
    /// battlefield may name different counters, and a permanent's counters are keyed by
    /// kind: collapsing them would let one card's ability spend the other's.
    Gold,
    /// A **wish** counter: the same shape again, on a permanent that enters with three.
    Wish,
    /// A **corpse** counter: a marker on a creature returned from a graveyard, whose
    /// only reader is the ability that put it there.
    Corpse,
}

/// An **emblem** (CR 114): a marker a player owns, whose only characteristics are its
/// abilities, and which exists outside every zone.
///
/// The last object model the engine was missing, and the only one with no removal path.
/// Everything else the engine creates is somewhere — a permanent is on the battlefield,
/// a card is in a zone, a stack object is on the stack — and every one of them has a
/// seam it leaves through. An emblem has none: nothing destroys, exiles, bounces, or
/// targets it, no state-based action collects it, and it survives the planeswalker whose
/// ultimate made it by an arbitrary number of turns. That is not a simplification; it is
/// CR 114.5, and it is why this type carries no zone, no tapped state, no counters, and
/// no damage.
///
/// It has **no [`PermanentId`]**, because it is not a permanent. Its [`id`](Self::id) is
/// minted from the same monotonic [`GameState::next_object_id`](crate::GameState) every
/// other object's is, so it is a unique, replay-stable handle *and* a CR 613.7 timestamp
/// — which is what lets an emblem's static ability fold into the layer system beside a
/// permanent's without the ordering code learning that emblems exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emblem {
    /// This emblem's object id, minted from
    /// [`GameState::next_object_id`](crate::GameState) — unique across every object in
    /// the game, and its CR 613.7 timestamp.
    pub id: u64,
    /// The player who has it (CR 114.2), and therefore the "you" its abilities are
    /// written from. An emblem is controlled by its owner and control never changes.
    pub controller: PlayerId,
    /// Its abilities, which are all it has (CR 114.1). Authored inline by the
    /// [`Effect::CreateEmblem`](crate::Effect) that made it, since an emblem is not a
    /// card and has no catalog entry to read them from.
    pub abilities: Vec<crate::ability::Ability>,
}

/// A permission to cast cards from a graveyard, granted for one turn
/// ([`Effect::AllowCastingFromGraveyard`](crate::Effect)).
///
/// **Raw stored state, not a derivation** (ADR 0005 §1): nothing else in
/// [`GameState`](crate::GameState) records that a player was given this, and a snapshot
/// of the zones could never recover it. Kept as a list rather than a flag because two
/// permissions can be in force at once and each names its own class of card.
///
/// It carries the [`turn`](Self::turn) it was granted on rather than a duration to tick
/// down: "this turn" is a fact about which turn it is, and comparing turn numbers cannot
/// drift the way a countdown can. The turn boundary drops every entry, so the list is
/// empty in almost every state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraveyardCasting {
    /// The player whose graveyard becomes castable.
    pub player: PlayerId,
    /// Which of that graveyard's cards may be cast.
    pub filter: crate::ability::CardFilter,
    /// The turn the permission was granted on; it lapses when that turn ends.
    pub turn: u32,
}

/// A permanent on the shared battlefield.
///
/// Its [`PermanentId`] is minted fresh on battlefield entry and is distinct
/// from the [`CardId`] of the card it represents. It also links the
/// [`CardInstanceId`] of the physical card it originated from, so identity is
/// preserved when the permanent leaves the battlefield for another zone.
///
/// Not every permanent is a card: a token (CR 111) is a battlefield object whose
/// characteristics come from the effect that created it, and [`Self::printed`] is
/// where that difference lives.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Permanent {
    /// Battlefield identity, fresh on entry.
    pub id: PermanentId,
    /// The physical card this permanent originated from. Stable across the zone
    /// change that put it here, unlike [`Self::id`].
    ///
    /// A token is given one too, minted from the same monotonic counter: it is the
    /// per-object handle the commander designation and the death diff are keyed on,
    /// and keeping it non-optional means neither has to special-case a token. Because
    /// a token never leaves the battlefield as a card (CR 111.7), that id can never
    /// turn up in a hand, a graveyard, or exile.
    pub instance: CardInstanceId,
    /// Where this permanent's printed characteristics come from: the catalog card it
    /// represents, or the token characteristics an effect gave it (CR 111).
    ///
    /// Read it through [`Printed::face`](crate::Printed::face), which answers both
    /// kinds; [`Printed::card`](crate::Printed::card) is the one accessor that crosses
    /// back to card identity, and its `None` for a token is what makes CR 111.7 —
    /// "a token that leaves the battlefield ceases to exist" — a thing the compiler
    /// asks about rather than a rule to remember.
    pub printed: Printed,
    /// The player who currently controls it.
    pub controller: PlayerId,
    /// Whether the permanent is tapped.
    pub tapped: bool,
    /// The turn number on which this permanent entered the battlefield under its
    /// current controller (came under their control). Raw stored state, set on
    /// battlefield entry from [`GameState::turn`](crate::GameState::turn); `0` for the empty [`Default`].
    ///
    /// This is half the fact "summoning sickness" is derived from (CR 302.6): a
    /// creature has been controlled continuously since its controller's most
    /// recent turn began exactly when it entered on an *earlier* turn than the one
    /// that controller most recently began — which, for every seat but the active
    /// player, is not the current turn. The other half is
    /// [`Player::turn_began`](crate::player::Player::turn_began). The engine cannot
    /// derive either from a bare snapshot — both are history — so, like
    /// [`Self::damage`], they are stored rather than computed.
    /// Not a zone-change counter: a permanent re-entering the battlefield gets a
    /// fresh [`PermanentId`] and a fresh `entered_turn`; nothing counts entries.
    pub entered_turn: u32,
    /// **What** this permanent is attacking — the player or planeswalker it was
    /// declared to attack this combat (CR 508.1a), or `None` if it is not attacking.
    /// Raw stored state, set when attackers are declared and cleared at the
    /// end-of-combat step (CR 511.3).
    ///
    /// This is the one field that carries combat's two generalizations. A two-player
    /// game's sole legal defender is the one opponent, but with more seats each
    /// attacker records *which* opponent it attacks (issue #341); and since issue #608
    /// it may record a **planeswalker** instead, whose controller is then the defending
    /// player for blocking purposes ([`AttackTarget::defending_player`]). Blocker
    /// eligibility and combat damage both follow this assignment. `None` for a
    /// permanent not in combat.
    pub attacking: Option<crate::combat::AttackTarget>,
    /// The attacker this permanent is blocking, if it was declared as a blocker
    /// this combat (CR 509.1); `None` for a permanent that is not blocking.
    ///
    /// A blocker is assigned to exactly one attacking creature (this field is that
    /// assignment); several blockers may name the same attacker. Raw stored state,
    /// set when blockers are declared and cleared at the end-of-combat step
    /// (CR 511.3).
    pub blocking: Option<PermanentId>,
    /// Whether this permanent skips its controller's **next** untap step (CR 502.4) —
    /// what `Those creatures don't untap during that player's next untap step` leaves
    /// behind after the spell that said it has gone.
    ///
    /// Raw stored state, and deliberately a **flag rather than a countdown**: the thing
    /// a card names is one specific untap step, so the flag is spent at the first untap
    /// step its controller reaches, whether or not the permanent is still tapped by
    /// then. A counter would have to be decremented somewhere, and anywhere it was not
    /// decremented the skip would silently outlive the card that granted it.
    ///
    /// It is *not* part of the computed characteristics: nothing about it is a
    /// continuous effect, it does not end at cleanup, and no layer applies to it. The
    /// permanent carries it the way it carries [`Self::damage`].
    pub skips_untap: bool,
    /// Damage marked on this permanent this turn (CR 120.3). Raw stored state,
    /// zeroed as a turn-based action during the cleanup step (CR 514.2) and,
    /// once combat lands (issue #118), compared against toughness by the
    /// state-based-actions loop (CR 704.5g). `0` means no marked damage.
    pub damage: u32,
    /// Counters on this permanent, keyed by [`CounterKind`] and mapped to how
    /// many of that kind are present.
    ///
    /// This is **raw stored state, not a derivation** (ADR 0005 §1): nothing
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
    /// **Raw stored state, not a derivation** (ADR 0005 §1): the attachment is a
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

/// A continuous static effect currently in force (ADR 0005 slice 3, §4).
///
/// This is **raw stored input, not a derivation** (ADR 0005 §1): the source
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
    /// `source` order (CR 613.7, ADR 0005 §4). No wall-clock and no ambient
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
    /// id (ADR 0005 §4 — the id assigned when the effect was created). Exposed as
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
    /// (ADR 0005 §3–§4).
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
    /// CR 613 **layer 6** (CR 613.1f): impose a combat restriction on the affected
    /// permanent — an Aura's "can neither attack nor block", or a spell's "target
    /// creature can't be blocked this turn". The exact counterpart of
    /// [`Self::GrantKeyword`] for the restriction vocabulary that is not keyworded
    /// ([`CombatRestriction`](crate::CombatRestriction)): idempotent, timestamp-
    /// independent, and folded into the permanent's computed restrictions by
    /// [`characteristics`](crate::characteristics::characteristics), so a granted
    /// restriction binds exactly as a printed one does.
    GrantRestriction(CombatRestriction),
}

/// One running total of cumulative **combat** damage a commander has dealt a
/// player over the game (CR 903.10a).
///
/// **Raw stored history, not a derivation** (ADR 0005 §1): "how much combat
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
