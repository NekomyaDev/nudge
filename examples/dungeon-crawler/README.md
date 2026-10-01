# Dungeon Crawler RPG

> [!NOTE]
> **Testing & Preview Stage / Test Aşaması**: This example is an experimental test fixture and technical preview evaluating pure Nudge state machines, routing semantics, and headless/graphical runtime interop.

A turn-based RPG dungeon crawler adventure developed purely in Nudge. Demonstrates stateful hero progression, turn-based combat with armor mitigation and critical strikes, equipment forging, merchant trading, and shrine restoration — without requiring external API keys.

## Features

- **Pure Nudge Logic**: Zero external dependencies, zero tokens, runs completely on the Nudge runtime.
- **Stateful Hero Progression**: State tracking for HP, Max HP, ATK, DEF, Potions, Gold, XP, Level, and Inventory.
- **Turn-based Combat**: Dynamic damage calculation with armor mitigation and critical hit multipliers.
- **In-Game Economy & Upgrades**: Potion purchasing, weapon forging, and shield reinforcement.
- **Environmental Shrines**: Holy shrines that miraculously restore player health.
- **Leveling System**: Experience points and level scaling that boost maximum health and combat attributes.
- **Test Suite**: Six comprehensive unit tests verifying game mechanics, damage math, and victory conditions.

## Running the Game

Check the program syntax:
```sh
nudgec check dungeon-crawler.ndg
```

Compile and run the adventure campaign:
```sh
nudgec build dungeon-crawler.ndg
PYTHONPATH=../../runtime python3 out/dungeon-crawler.py
```

Compile to TypeScript:
```sh
nudgec build-ts dungeon-crawler.ndg
```

Run unit tests:
```sh
nudgec test dungeon-crawler.ndg
```

### Visual Web UI (Graphical Mode)
Play directly with full graphics, health bars, inventory slots, 8-bit sound effects, and animations:
```sh
# Open in your default browser:
xdg-open index.html

# Or serve locally:
python3 -m http.server 8080
# then visit http://localhost:8080/index.html
```

### Interactive CLI Mode
```sh
python3 play.py
```

