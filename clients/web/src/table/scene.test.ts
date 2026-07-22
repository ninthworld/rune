import { describe, expect, it } from "vitest";
import { normalizeGameView } from "../wire";
import { SAMPLE_GAME_VIEW } from "../game-view.fixture";
import type { GameView } from "../protocol";
import { deriveColorIdentity } from "./colorIdentity";
import { build, GEO, boardView } from "./scene.fixture";

describe("deriveColorIdentity", () => {
  it("frames any land as land regardless of cost", () => {
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Forest",
        type_line: "Basic Land — Forest",
      }),
    ).toBe("L");
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Ancient Tomb",
        type_line: "Land",
        mana_cost: "{2}",
      }),
    ).toBe("L");
  });

  it("reads a single color from the mana cost", () => {
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Bears",
        type_line: "Creature",
        mana_cost: "{1}{G}",
      }),
    ).toBe("G");
  });

  it("marks two or more colors as multicolor", () => {
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Bolt",
        type_line: "Instant",
        mana_cost: "{W}{U}",
      }),
    ).toBe("M");
  });

  it("treats hybrid pips as the colors they name", () => {
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Hybrid",
        type_line: "Creature",
        mana_cost: "{W/U}",
      }),
    ).toBe("M");
  });

  it("falls back to colorless for generic-only or absent costs", () => {
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Sol Ring",
        type_line: "Artifact",
        mana_cost: "{1}",
      }),
    ).toBe("C");
    expect(
      deriveColorIdentity({
        id: "x",
        name: "Ornithopter",
        type_line: "Artifact Creature",
      }),
    ).toBe("C");
  });
});

describe("buildTableScene local player", () => {
  it("identifies the receiver straight from view.you", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.localPlayerId).toBe("p1");
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
  });

  it("resolves the local band at game start, before any public zone exists", () => {
    // The heuristic this replaces returned undefined on an empty opening board;
    // `view.you` names the receiver even with nothing on the table yet.
    const opening: GameView = {
      ...SAMPLE_GAME_VIEW,
      you: "p1",
      battlefield: [],
      graveyards: [],
      exile: [],
      priority_player: undefined,
    };
    const scene = build(opening);
    expect(scene.localPlayerId).toBe("p1");
    // A local band is still laid out for the receiver even with no permanents.
    expect(scene.bands.map((b) => b.playerId)).toEqual(["p2", "p1"]);
    expect(scene.bands.at(-1)?.isLocal).toBe(true);
  });

  it("treats an absent view.you (older server) as unknown", () => {
    const legacy = normalizeGameView({
      ...JSON.parse(JSON.stringify(SAMPLE_GAME_VIEW)),
      you: "",
    });
    const scene = build(legacy);
    expect(scene.localPlayerId).toBeUndefined();
    // No band is flagged local when the receiver is unknown.
    expect(scene.bands.every((b) => !b.isLocal)).toBe(true);
  });
});

describe("buildTableScene", () => {
  it("groups the battlefield into per-controller bands with the local band last", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.bands.map((b) => b.playerId)).toEqual(["p2", "p1"]);
    const local = scene.bands.at(-1);
    expect(local?.isLocal).toBe(true);
    expect(local?.cards.map((c) => c.entityId)).toEqual(["perm_xyz"]);
  });

  it("passes P/T, tapped and counters through verbatim (no game logic)", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    expect(bear?.data.power).toBe("2");
    expect(bear?.data.toughness).toBe("2");
    expect(bear?.data.tapped).toBe(true);
    expect(bear?.data.counters).toEqual([{ kind: "+1/+1", count: 2 }]);
    expect(bear?.data.colorIdentity).toBe("G");
  });

  it("routes each subject-action onto its entity, leaving others non-interactive", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const bear = scene.bands.at(-1)?.cards[0];
    // The activate-ability action names perm_xyz, so it rides on the card.
    expect(bear?.actions.map((a) => a.id)).toEqual(["a2"]);
    // The hand card has no subject-action → no on-entity interactivity.
    expect(scene.hand[0]?.entityId).toBe("c1");
    expect(scene.hand[0]?.actions).toEqual([]);
  });

  it("renders the local hand at hand tier", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.hand.map((c) => c.tier)).toEqual(["hand"]);
  });

  it("labels each band by its controller and marks the local one (issue #278)", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const local = scene.bands.at(-1);
    const opponent = scene.bands[0];
    expect(local?.isLocal).toBe(true);
    expect(local?.label).toBe("p1 (you)");
    expect(opponent?.label).toBe("p2");
  });

  it("gives every band a bounded region, including an empty one (issue #278)", () => {
    const scene = build(boardView(["p1"], 0));
    const band = scene.bands[0];
    expect(band?.isEmpty).toBe(true);
    // An empty panel still reserves a carved, non-zero home a newcomer can see.
    expect(band?.rect.w).toBeGreaterThan(0);
    expect(band?.rect.h).toBeGreaterThan(0);
  });

  it("carries each controller's zone pile counts straight from the view (issue #278)", () => {
    const view = SAMPLE_GAME_VIEW;
    const scene = build(view);
    const local = scene.bands.at(-1);
    const opponent = scene.bands[0];
    // Local library comes from `me`; an opponent's from its redacted view.
    expect(local?.zones.library).toBe(view.me.library_size);
    expect(opponent?.zones.library).toBe(
      view.opponents.find((o) => o.player_id === "p2")?.library_size ?? -1,
    );
    // Graveyard/exile counts mirror the piles the tiles read.
    expect(local?.zones.graveyard).toBe(
      view.graveyards.find((g) => g.player_id === "p1")?.cards.length ?? -1,
    );
  });

  it("labels the hand row as its own region (issue #278)", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    expect(scene.handRegion.label).toBe("Your hand");
    expect(scene.handRegion.rect.h).toBeGreaterThan(0);
    expect(scene.handRegion.rect).toEqual(GEO.hand);
  });

  it("marks the selected entity so its card draws a ring", () => {
    const scene = build(SAMPLE_GAME_VIEW, "perm_xyz");
    expect(scene.bands.at(-1)?.cards[0]?.data.selected).toBe(true);
    expect(scene.hand[0]?.data.selected).toBe(false);
  });

  it("marks a card with offered actions as actionable and inert cards not (issue #277)", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    // perm_xyz carries the activate-ability action → the playable affordance.
    expect(scene.bands.at(-1)?.cards[0]?.data.actionable).toBe(true);
    // The hand card has no subject-action → no affordance, purely from the view.
    expect(scene.hand[0]?.data.actionable).toBe(false);
  });

  it("is a pure function of its inputs: identical view → identical scene", () => {
    const a = build(SAMPLE_GAME_VIEW, "perm_xyz");
    const b = build(SAMPLE_GAME_VIEW, "perm_xyz");
    expect(a).toEqual(b);
  });

  it("leaves nothing targetable outside targeting mode", () => {
    const scene = build(SAMPLE_GAME_VIEW);
    const all = [...scene.bands.flatMap((b) => b.cards), ...scene.hand];
    expect(all.every((c) => c.targetable === false)).toBe(true);
    expect(
      all.every(
        (c) => c.data.targeting === undefined || c.data.targeting === false,
      ),
    ).toBe(true);
  });
});
