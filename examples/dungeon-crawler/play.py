#!/usr/bin/env python3
"""Interactive RPG Runner for The Crypt of Shadows (powered by Nudge)"""
import sys
import os

repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(repo_root, "runtime"))

import importlib.util
out_file = os.path.join(repo_root, "out", "dungeon-crawler.py")
if not os.path.exists(out_file):
    import subprocess
    subprocess.run(["nudgec", "build", "dungeon-crawler.ndg"], cwd=os.path.dirname(__file__), check=True)

spec = importlib.util.spec_from_file_location("dungeon_crawler", out_file)
game = importlib.util.module_from_spec(spec)
spec.loader.exec_module(game)

def print_status():
    print(f"\n[Hero Status] Level: {game.get_level()} | HP: {game.get_hp()}/{game.get_max_hp()} | ATK: {game.get_atk()} | DEF: {game.get_def()} | Gold: {game.get_gold()} | Potions: {game.get_potions()}")

def interactive():
    print("============================================================")
    print("      THE CRYPT OF SHADOWS — INTERACTIVE ADVENTURE          ")
    print("                 Powered by Pure Nudge                      ")
    print("============================================================")
    game.reset_hero()
    print_status()

    while game.is_hero_alive() and not game.is_boss_slain():
        print("\nChoose your action:")
        print("  1. Explore / Battle current room")
        print("  2. Drink healing potion (+40 HP)")
        print("  3. Buy potion from merchant (10 Gold)")
        print("  4. Blacksmith: Forge weapon (+8 ATK, 20 Gold)")
        print("  5. Blacksmith: Reinforce armor (+4 DEF, 15 Gold)")
        print("  6. Pray at the Sunken Shrine (Full Heal)")
        print("  7. Run complete automated campaign walkthrough")
        print("  8. Show status")
        print("  0. Quit game")

        try:
            choice = input("\nEnter choice [0-8]: ").strip()
        except (EOFError, KeyboardInterrupt):
            print("\nExiting.")
            break

        if choice == "1":
            room = game.get_room() + 1
            game._state_DungeonHero.current_room = room
            title = game.room_title(room)
            print(f"\n>>> Entering Room {room}: {title} <<<")
            if room == 1:
                won = game.fight_encounter(22, 10, 2, 15, 30, False)
            elif room == 2:
                won = game.fight_encounter(36, 12, 4, 30, 50, True)
            elif room == 3:
                print("You reached the subterranean workshop! You can trade or forge gear.")
                continue
            elif room == 4:
                print("You found the holy shrine! Choose option 6 to pray.")
                continue
            elif room == 5:
                won = game.fight_encounter(65, 20, 6, 100, 150, True)
                game._state_DungeonHero.boss_defeated = won
            else:
                print("All rooms cleared! Returning to surface.")
                break

            if not won:
                print("\n[!] The hero has fallen in battle! Game Over.")
                break
            else:
                print(f"[+] Victory in {title}!")
                print_status()

        elif choice == "2":
            new_hp = game.drink_potion()
            print(f"Drank potion. Current HP: {new_hp}/{game.get_max_hp()} (Potions left: {game.get_potions()})")
        elif choice == "3":
            if game.buy_potion(10):
                print(f"Purchased potion! (Potions: {game.get_potions()}, Gold: {game.get_gold()})")
            else:
                print("Not enough gold!")
        elif choice == "4":
            new_atk = game.forge_weapon(20, 8)
            print(f"Weapon upgraded! New ATK: {new_atk} (Gold: {game.get_gold()})")
        elif choice == "5":
            new_def = game.reinforce_armor(15, 4)
            print(f"Armor reinforced! New DEF: {new_def} (Gold: {game.get_gold()})")
        elif choice == "6":
            game.pray_at_shrine()
            print(f"Prayed at shrine! Full HP restored: {game.get_hp()}/{game.get_max_hp()}")
        elif choice == "7":
            print("\n" + game.run_campaign())
            break
        elif choice == "8":
            print_status()
        elif choice == "0":
            print("Exiting game.")
            break
        else:
            print("Invalid choice, please select 0-8.")

    if game.is_boss_slain():
        print("\nCONGRATULATIONS! You have conquered the Crypt of Shadows!")

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--campaign":
        print(game.run_campaign())
    else:
        interactive()
