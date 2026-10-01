# Dungeon Crawler RPG

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
