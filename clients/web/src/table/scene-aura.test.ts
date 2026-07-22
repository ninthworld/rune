import { describe, expect, it } from "vitest";
import { build, permBoard } from "./scene.fixture";

describe("buildTableScene aura clustering (issue #333)", () => {
  it("clusters an aura adjacent to its host, host first, in the host's row", () => {
    const local = build(
      permBoard([
        {
          id: "bear",
          name: "Grizzly Bears",
          type_line: "Creature — Bear",
          power: "2",
          toughness: "2",
        },
        {
          id: "aura",
          name: "Ironbark Aegis",
          type_line: "Enchantment — Aura",
          attached_to: "bear",
        },
      ]),
    ).bands.at(-1)!;
    // The aura leaves the support row and rides in the host's creatures row, right
    // after the host, so the two read as one cluster.
    const creatures = local.rows.find((r) => r.kind === "creatures")!;
    const inCreatures = local.cards.filter(
      (c) => c.rect.y === creatures.rect.y,
    );
    expect(inCreatures.map((c) => c.entityId)).toEqual(["bear", "aura"]);
    // No standalone support row is created for the clustered aura.
    expect(local.rows.some((r) => r.kind === "support")).toBe(false);
    expect(local.cards.find((c) => c.entityId === "aura")!.attachedTo).toBe(
      "bear",
    );
    expect(local.cards.find((c) => c.entityId === "bear")!.attachments).toEqual(
      ["aura"],
    );
  });

  it("never folds an attachment or its host into an ×N stack", () => {
    // Two identical bears; only one is enchanted. Without clustering they would fold
    // into a ×2 — the enchanted host and its aura must stay their own renders.
    const local = build(
      permBoard([
        {
          id: "bear_a",
          name: "Grizzly Bears",
          type_line: "Creature — Bear",
          power: "2",
          toughness: "2",
        },
        {
          id: "bear_b",
          name: "Grizzly Bears",
          type_line: "Creature — Bear",
          power: "2",
          toughness: "2",
          attached_to: undefined,
        },
        {
          id: "aura",
          name: "Ironbark Aegis",
          type_line: "Enchantment — Aura",
          attached_to: "bear_a",
        },
      ]),
    ).bands.at(-1)!;
    const host = local.cards.find((c) => c.entityId === "bear_a")!;
    expect(host.stackCount).toBe(1);
    const aura = local.cards.find((c) => c.entityId === "aura")!;
    expect(aura.stackCount).toBe(1);
    // The un-enchanted bear is still individually present (it has nothing to fold with).
    expect(local.cards.some((c) => c.entityId === "bear_b")).toBe(true);
  });

  it("keeps a clustered attachment individually addressable in targeting mode", () => {
    const scene = build(
      permBoard([
        {
          id: "bear",
          name: "Grizzly Bears",
          type_line: "Creature — Bear",
          power: "2",
          toughness: "2",
        },
        {
          id: "aura",
          name: "Ironbark Aegis",
          type_line: "Enchantment — Aura",
          attached_to: "bear",
        },
      ]),
      undefined,
      { candidates: ["aura"] },
    );
    const aura = scene.bands.at(-1)!.cards.find((c) => c.entityId === "aura")!;
    expect(aura.targetable).toBe(true);
    expect(aura.stackCount).toBe(1);
  });

  it("degrades gracefully when the referenced host is not in the visible battlefield", () => {
    // The host is not on the board (e.g. an aura on an object the viewer cannot see):
    // the aura renders in its own support row exactly as an unattached permanent would.
    const local = build(
      permBoard([
        {
          id: "aura",
          name: "Pacifism",
          type_line: "Enchantment — Aura",
          attached_to: "ghost",
        },
      ]),
    ).bands.at(-1)!;
    expect(local.rows.map((r) => r.kind)).toEqual(["support"]);
    const aura = local.cards.find((c) => c.entityId === "aura")!;
    expect(aura.attachedTo).toBeUndefined();
  });

  it("reconstructs identical clustering from one GameView (fresh mount)", () => {
    const view = permBoard([
      {
        id: "bear",
        name: "Grizzly Bears",
        type_line: "Creature — Bear",
        power: "2",
        toughness: "2",
      },
      {
        id: "aura",
        name: "Ironbark Aegis",
        type_line: "Enchantment — Aura",
        attached_to: "bear",
      },
    ]);
    expect(build(view)).toEqual(build(view));
  });
});
