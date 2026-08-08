#!/usr/bin/env python3
"""Transcribe a set's printed characteristics into the fixture the catalog is checked against.

Usage:  scripts/printed-characteristics.py <mtgjson-set.json> [<mtgjson-set.json> ...]

Writes crates/sage-engine/tests/fixtures/printed_characteristics.json: one record per
card the bundled catalog holds, carrying only what is printed in the type line and the
mana cost.  No rules text, no flavour text, no artist, no image, no set symbol -- the
functional characteristics ADR 0009 already sources, and nothing the licensing rule in
AGENTS.md forbids.

The point of this file is that it is transcribed from the *printed set* and never from
the catalog.  Regenerating it from `crates/sage-engine/data/catalog/` would turn the gate
that reads it into a no-op.
"""
import json, glob, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CATALOG = os.path.join(ROOT, "crates", "sage-engine", "data", "catalog")
OUT = os.path.join(ROOT, "crates", "sage-engine", "tests", "fixtures",
                   "printed_characteristics.json")

COLOR = {"W": "white", "U": "blue", "B": "black", "R": "red", "G": "green"}
WUBRG = ["white", "blue", "black", "red", "green"]


def faces(cards):
    """Every printing in the source, indexed by the name a catalog entry would use."""
    by_name = {}
    for card in cards:
        by_name.setdefault(card["name"], []).append(card)
        if card.get("faceName"):
            by_name.setdefault(card["faceName"], []).append(card)
    return by_name


def characteristics(card):
    out = {
        "name": card.get("faceName") or card["name"],
        "mana_cost": card.get("manaCost", ""),
        "types": [t.lower() for t in card.get("types", [])],
        "colors": [c for c in WUBRG if c in {COLOR[x] for x in card.get("colors", [])}],
    }
    if card.get("supertypes"):
        out["supertypes"] = [t.lower() for t in card["supertypes"]]
    if card.get("subtypes"):
        out["subtypes"] = list(card["subtypes"])
    if card.get("power") is not None:
        out["power"] = card["power"]
    if card.get("toughness") is not None:
        out["toughness"] = card["toughness"]
    if card.get("loyalty") is not None:
        out["loyalty"] = card["loyalty"]
    return out


def main(paths):
    cards = []
    for path in paths:
        cards.extend(json.load(open(path))["data"]["cards"])
    by_name = faces(cards)

    records = []
    missing = []
    for path in sorted(glob.glob(os.path.join(CATALOG, "*.json"))):
        entry = json.load(open(path))
        printings = by_name.get(entry["name"])
        if not printings:
            missing.append(entry["functional_id"])
            continue
        front = next((c for c in printings if c.get("side") in (None, "a")), printings[0])
        record = {"functional_id": entry["functional_id"]}
        record.update(characteristics(front))
        if front.get("layout") == "transform":
            back = next(c for c in cards
                        if c["name"] == front["name"] and c.get("side") == "b")
            record["back_face"] = characteristics(back)
        records.append(record)

    if missing:
        sys.exit(f"no printing found for: {', '.join(missing)}")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as handle:
        json.dump(records, handle, indent=2)
        handle.write("\n")
    print(f"wrote {OUT} ({len(records)} cards)")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
