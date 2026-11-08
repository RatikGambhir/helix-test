# RuneCraft

A multiplayer block-building sandbox written in Rust with the [Rune](https://github.com/anomalyco/rune) UI framework. Mine, craft, build and survive in an endless world, and visit block-built versions of **New York, Tokyo, Chicago, London, Paris, Sydney, Dubai and Rome** with your friends.

![New York](docs/newyork.jpg)

| | |
|---|---|
| ![Tokyo at night](docs/night.jpg) | ![Chicago and the L](docs/chicago.jpg) |
| ![Inventory, crafting, recipe book](docs/inventory.jpg) | ![Map and city travel](docs/map.jpg) |
| ![The Colosseum](docs/colosseum.jpg) | ![The Opera House](docs/opera.jpg) |
| ![Harbour Bridge](docs/hbridge.jpg) | ![Tower Bridge](docs/towerbridge.jpg) |
| ![A friend joins](docs/friends.jpg) | ![Rain](docs/rain.jpg) |
| ![Creative catalog](docs/creative.jpg) | ![Title screen](docs/title.jpg) |

## Play

You need Rust ([rustup](https://rustup.rs)); the right toolchain installs itself from `rust-toolchain.toml`. On Linux you also need two development packages: `sudo apt install libxkbcommon-dev libasound2-dev` (Debian/Ubuntu).

```sh
cargo run --release
```

The first build takes a few minutes. Use `--release`: debug builds are noticeably slower in game.

On the title screen pick your name and look, choose Survival or Creative for new worlds, then:

- **Create & play** makes a world on your computer. Your worlds are listed there next time (saved in `~/.runecraft/worlds`).
- **Create on LAN** / **LAN** opens a world your friends on the same network can join. Your LAN address shows in the top-right corner in game.
- **Join server** connects to a friend's LAN game or a dedicated server (`host` or `host:port`, default port 25575).

### Dedicated server

```sh
cargo run --release -p runecraft-server -- --port 25575 --world worlds/main --seed 1234
```

Options: `--port`, `--world <dir>`, `--seed <n>` (new worlds only), `--view <chunks>`, `--motd <text>`, `--no-mobs`. Press Enter in the terminal to save and stop. To play over the internet, forward TCP port 25575 on your router to the server machine.

## Controls

| Key | Action |
|---|---|
| W A S D | Walk; Ctrl to sprint, Shift to sneak |
| Mouse | Look around: click into the game to grab the mouse, Esc to let go |
| Left click (hold) | Mine; attack creatures (hit while falling for a critical) |
| Right click | Place blocks; open doors, chests, furnaces and crafting tables; sleep in beds; eat; shoot a bow; till with a hoe; plant seeds; use buckets and bone meal; talk to citizens; light TNT |
| Space | Jump, swim up, climb ladders; double-tap to fly in Creative |
| 1–9, mouse wheel | Choose a hotbar slot |
| E | Inventory and crafting |
| Q / Ctrl+Q | Drop one item / the whole stack |
| M | Map and travel to cities |
| T, Enter, / | Chat and commands |
| F5 | First person / third person behind / in front |
| N | Minimap |
| F3 | Debug info |
| Esc | Pause menu: settings, controls, game mode, quit |

In menus: click to pick up and put down stacks, right-click to split a stack or place one item, shift-click to move a stack across. The recipe book crafts anything you have the ingredients for in one click.

Commands: `/tp chicago`, `/tp x y z`, `/tp <player>`, `/spawn`, `/time day|night|noon|midnight`, `/weather clear|rain|thunder`, `/give <item> [count]`, `/gamemode creative|survival`, `/fly`, `/kill`, `/list`, `/cities`, `/seed`, `/help`.

On Wayland the system doesn't let apps move the pointer, so mouse capture is off and you look around by dragging instead (Settings shows which mode is active).

## What's in the game

**World.** Endless terrain from a seed: plains, forests, birch forests, cherry groves, deserts, snowy tundra, mountains, oceans and beaches; caves with lava; coal, copper, iron, gold and diamond ores by depth; trees, grass and flowers. Chunks generate on server threads and stream to players.

**Cities.** Built into every world at fixed places, with street grids, sidewalks, street lamps, parked cars, parks with trees and ponds, and buildings you can walk into (lobbies, floors and lit windows).
- **New York**: skyscrapers with setbacks and water towers, Times Square neon, yellow cabs, Central Park, the Empire tower, One World Trade, a river around Manhattan with a stone suspension bridge, and the Liberty statue on her island. Subway tunnels under the avenues.
- **Tokyo**: neon streets, the scramble crossing, Tokyo Tower, the Skytree and a cherry-blossom shrine. Subway.
- **Chicago**: the Loop with elevated L tracks, the river with bridges, the tallest tower in town, Millennium Park and its bean, and Navy Pier on Lake Michigan. Subway.
- **London**: brick terraces, red cars, the Thames, the clock tower and parliament, Tower Bridge, the big wheel and the glass shard. Underground.
- **Paris**: limestone boulevards with slate mansard roofs, the iron tower, the triumphal arch and the Louvre pyramid. Métro.
- **Sydney**: the Opera House on its harbour podium and the Harbour Bridge arch.
- **Dubai**: glass and gold towers and the tallest building in the world over a reflecting pool.
- **Rome**: terracotta streets, umbrella pines and the Colosseum, half of its top ring fallen.

**Survival.** Health, hunger, eating, regeneration, fall damage, drowning, lava and cactus damage, death that scatters your items, respawn at your bed. Mining time depends on hardness and tool; ores need the right pickaxe.

**Items and crafting.** 78 kinds of blocks and 58 items, all with textures painted in code. A real inventory with an armor column, cursor stacks, splitting and quick-moving; shaped crafting on the 2×2 grid and the 3×3 crafting table (mirrored recipes work too); a recipe book; furnaces with fuel for smelting and cooking; chests. Tools (pickaxe, axe, shovel, hoe, sword in wood, stone, iron and diamond) wear out. Iron and diamond armor, bows and arrows, buckets, bone meal, food.

**Building.** Doors, ladders you can climb, beds, torches, lanterns, glass, rails, TNT that chain-explodes, flowing water and lava (they make obsidian and cobblestone where they meet), farms with tilled soil, wheat that grows, and saplings that grow into trees.

**Creatures.** Pigs, cows, sheep and chickens with drops; zombies, skeletons that shoot arrows and leaping spiders at night and in dark caves (zombies and skeletons burn in daylight); citizens who walk the city sidewalks and chat when you right-click them.

**World life.** Day and night with sunsets, stars, a moon and drifting clouds; rain, snow in cold places, thunderstorms with lightning; smooth lighting and ambient occlusion, warm torch light, glowing neon; procedurally synthesised sound effects (digging, steps, creatures, explosions, thunder) and gentle ambient music.

**Multiplayer.** Every player sees the same world, blocks, creatures, items, weather and time; avatars animate with name tags and show what they hold; chat; the server saves edits, chests, furnaces and each player's inventory, health, mode, position and spawn point.

**Creative.** Unlimited blocks, a searchable catalog of everything, instant breaking, flight, no damage.

## Not in this version yet

From the bigger vision, these aren't built yet: real-world map data (OpenStreetMap streets and building footprints, elevation), vehicles, trains and boats, redstone-style circuits, other dimensions, villages and trading, experience and enchanting, more creature types and breeding, greedy meshing and distant level-of-detail. The cities are hand-designed in code, not traced from map data.

## How it's built

```
crates/common   Rules shared by client and server: blocks and items, world generation, cities,
                physics, crafting, and the network protocol.
crates/server   The authoritative server: chunks, players, creatures, items, arrows, fluids,
                crops, furnaces, weather, saving. Also runs inside the client for single player.
crates/client   The game: a Rune app with its own 3D renderer, mesher, sound and map.
```

**Rune** draws all the 2D interface (title screen, HUD, hotbar, inventory, crafting, containers, map, chat, settings) and handles keyboard and pointer input. Rune is a 2D UI framework, so the 3D world is drawn by RuneCraft's own wgpu renderer, plugged in through Rune's `Compositor` hook (`crates/client/src/render.rs`): each frame it draws the sky, chunk meshes, water, creatures, particles and your hand, then asks Rune to draw the interface on top. Rune has no pointer lock, so `capture.rs` re-centres the hidden cursor itself on X11, Windows and macOS.

The world is stored in 16×16×256 columns, generated on server threads and streamed run-length encoded over TCP (length-prefixed bincode). The client meshes 16³ sections on worker threads with flood-filled sky and block light, smooth lighting and ambient occlusion.

## Tests

```sh
cargo test --workspace
```

Tests cover blocks, crafting recipes, world generation, cities, physics, the protocol, the server (join, chunks, edits, saving, dropped items coming back), flowing water, meshing, input routing through Rune's headless runtime, chat focus, and a two-player session (join, see each other, walk, break a block, chat).

The game can render itself without a window, which is how the screenshots above were made: `cargo run -- --snap shot.png --scene chicago` (scenes: `play`, `newyork`, `tokyo`, `paris`, `london`, `chicago`, `sydney`, `dubai`, `rome`, `night`, `rain`, `friends`, `inventory`, `creative`, `map`), or any camera with `RUNECRAFT_AT="x,y,z,yaw,pitch" cargo run -- --snap shot.png --scene at`.

## License

MIT
