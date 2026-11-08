# RuneCraft

A multiplayer block-building sandbox written in Rust with the [Rune](https://github.com/anomalyco/rune) UI framework, for macOS, Windows and Linux. Mine, craft, build and survive in an endless world, and visit block-built versions of **New York, Tokyo, Chicago, London, Paris, Sydney, Dubai and Rome** with your friends. Download **any real place on Earth** from OpenStreetMap, drive cars and trains, wire up redstone, trade in villages, and cross portals into the Ember and Sky realms.

![New York](docs/newyork.jpg)

| | |
|---|---|
| ![Chicago and the L](docs/chicago.jpg) | ![One World Trade](docs/wtc.jpg) |
| ![Inventory, crafting, recipe book](docs/inventory.jpg) | ![Map and city travel](docs/map.jpg) |
| ![The Colosseum](docs/colosseum.jpg) | ![The Opera House](docs/opera.jpg) |
| ![Harbour Bridge](docs/hbridge.jpg) | ![Tower Bridge](docs/towerbridge.jpg) |
| ![A house built in game](docs/house.jpg) | ![A friend joins](docs/friends.jpg) |
| ![Rain](docs/rain.jpg) | ![Tokyo at night](docs/night.jpg) |
| ![A village](docs/village.jpg) | ![Trading post](docs/trade.jpg) |
| ![Cars in New York](docs/cars.jpg) | ![A train](docs/train.jpg) |
| ![Redstone](docs/redstone.jpg) | ![Real places on the map](docs/map.jpg) |
| ![The Ember realm](docs/ember.jpg) | ![The Sky realm](docs/sky.jpg) |
| ![Creative catalog](docs/creative.jpg) | ![Title screen](docs/title.jpg) |

## Play

### On a Mac

Download **RuneCraft-macOS.zip** from the latest successful run of the [Build workflow](../../actions/workflows/build.yml) (scroll to *Artifacts*), unzip it and drag **RuneCraft.app** to Applications. It runs on Apple Silicon and Intel Macs (macOS 11 or later).

The app isn't notarised by Apple, so the first time, right-click it and choose **Open**, then **Open** again. (Or in Terminal: `xattr -dr com.apple.quarantine /Applications/RuneCraft.app`.)

To build it yourself on a Mac, install Rust ([rustup](https://rustup.rs)) and run `packaging/macos/build-app.sh`; it writes `target/RuneCraft.app` and a zip that also holds the dedicated server.

### From source (any platform)

You need Rust ([rustup](https://rustup.rs)); the right toolchain installs itself from `rust-toolchain.toml`. On Linux you also need two development packages: `sudo apt install libxkbcommon-dev libasound2-dev` (Debian/Ubuntu).

```sh
cargo run --release
```

The first build takes a few minutes. On a Mac this runs the same game without the app bundle. Use `--release`: debug builds are noticeably slower in game.

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
| Left click (hold) | Mine |
| Right click | Place blocks; open doors, chests, furnaces, crafting tables and trading posts; flip levers, press buttons; sleep in beds; eat; till with a hoe; plant seeds; use buckets; light TNT and portals with flint and steel; get into vehicles |
| In a vehicle | W / S to drive and brake (a train reverses once stopped), A / D to steer, Shift to get out |
| Space | Jump, swim up, climb ladders; double-tap to fly in Creative |
| 1–9, mouse wheel | Choose a hotbar slot |
| E | Inventory and crafting |
| Q / Ctrl+Q | Drop one item / the whole stack |
| M | Map: travel to cities, download real places |
| T, Enter, / | Chat and commands |
| F5 | First person / third person behind / in front |
| N | Minimap |
| F3 | Debug info |
| Esc | Pause menu: settings, controls, game mode, quit |

In menus: click to pick up and put down stacks, right-click to split a stack or place one item, shift-click to move a stack across. The recipe book crafts anything you have the ingredients for in one click.

Commands: `/tp chicago`, `/tp x y z`, `/tp <player>`, `/spawn`, `/time day|night|noon|midnight`, `/weather clear|rain|thunder`, `/give <item> [count]`, `/gamemode creative|survival`, `/fly`, `/kill`, `/list`, `/cities`, `/seed`, `/place <any place>`, `/places`, `/tp place <n>`, `/realm overworld|ember|sky`, `/help`.

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

**Real places.** Press M, type any place ("Times Square, New York", "Shibuya", or "48.858, 2.294") and press Build. The game looks it up on OpenStreetMap, downloads the streets, buildings (with their real heights), parks, water and railways within 300 m, and builds them 1 block = 1 metre far out east in your world, then takes you there. Each place is saved with the world and listed on the map for everyone on the server. This needs an internet connection (the server does the download), uses the public Nominatim and Overpass services, and takes from a few seconds to a minute. Map data © OpenStreetMap contributors, under the Open Database License.

**Villages and trading.** Villages of houses, farms, a well and a market square appear in plains, forests and deserts. Nobody lives in them. You trade at the trading posts under the market awnings: wheat, coal, iron or logs for emeralds, and emeralds for food, rails, redstone, a minecart, a boat, a car, a train or diamonds. Emerald ore is in the mountains and the Sky realm.

**Vehicles.** Minecarts and trains ride rails (rails join up, curve and climb slopes by themselves); a train is an engine pulling three cars, with a seat for a friend in each. Powered rails speed carts up when they're switched on. Boats float and steer on water; cars drive on any ground, climb kerbs and fit right in on the city avenues. Craft them, put them down, right-click to get in.

**Redstone.** Redstone dust (from redstone ore) carries power up to 15 blocks, up and down steps. Levers, buttons, pressure plates, redstone torches (which invert, so you can make clocks and logic) and blocks of redstone power it; it lights redstone lamps, opens doors, sets off TNT and switches powered rails.

**The realms.** Build a frame of obsidian (4 wide, 5 tall), light it with flint and steel and step in to reach the **Ember realm**: red caverns over a sea of lava, with quartz, ash sand and glowing clusters. Every block there is eight in the overworld. A frame of glowstone leads to the **Sky realm**: floating islands of grass and cloudstone with cherry trees. A portal is built for you on the other side if there isn't one.

**Survival.** Health, hunger, eating (apples from oak leaves, bread from wheat you farm), regeneration, fall damage, drowning, lava and cactus damage, death that scatters your items, respawn at your bed. Mining time depends on hardness and tool; ores need the right pickaxe.

**Items and crafting.** 90 kinds of blocks and 44 items, all with textures painted in code. A real inventory with an armor column, cursor stacks, splitting and quick-moving; shaped crafting on the 2×2 grid and the 3×3 crafting table (mirrored recipes work too); a recipe book; furnaces with fuel for smelting; chests. Tools (pickaxe, axe, shovel, hoe in wood, stone, iron and diamond) wear out. Iron and diamond armor, buckets, food.

**Building.** Doors, ladders you can climb, beds, torches, lanterns, glass, rails, TNT that chain-explodes, flowing water and lava (they make obsidian and cobblestone where they meet), farms with tilled soil, wheat that grows, and saplings that grow into trees.

**No creatures.** The world has no mobs, animals or AI characters: it's yours to build in, alone or with friends.

**World life.** Day and night with sunsets, stars, a moon and drifting clouds; rain, snow in cold places, thunderstorms with lightning; smooth lighting and ambient occlusion, warm torch light, glowing neon; procedurally synthesised sound effects (digging, steps, explosions, thunder) and gentle ambient music.

**Multiplayer.** Every player sees the same world, blocks, dropped items, weather and time; avatars animate with name tags and show what they hold; chat; the server saves edits, chests, furnaces and each player's inventory, health, mode, position and spawn point.

**Creative.** Unlimited blocks, a searchable catalog of everything, instant breaking, flight, no damage.

## Not in this version yet

From the bigger vision, these aren't built yet: elevation in real places (they're built flat), pistons and repeaters, experience and enchanting, greedy meshing and distant level-of-detail. The eight built-in cities are hand-designed in code; real places come from map data. Creatures, NPCs, villagers and combat are left out on purpose.

## How it's built

```
crates/common   Rules shared by client and server: blocks and items, world generation (overworld,
                villages, the two realms), cities, OpenStreetMap conversion, vehicle physics,
                crafting, and the network protocol.
crates/server   The authoritative server: chunks for three dimensions, players, dropped items,
                fluids, crops, furnaces, redstone, portals, vehicles, place downloads, weather,
                saving. Also runs inside the client for single player.
crates/client   The game: a Rune app with its own 3D renderer, mesher, sound and map.
```

**Rune** draws all the 2D interface (title screen, HUD, hotbar, inventory, crafting, containers, map, chat, settings) and handles keyboard and pointer input. Rune is a 2D UI framework, so the 3D world is drawn by RuneCraft's own wgpu renderer, plugged in through Rune's `Compositor` hook (`crates/client/src/render.rs`): each frame it draws the sky, chunk meshes, water, players, dropped items, particles and your hand, then asks Rune to draw the interface on top. Rune has no pointer lock, so `capture.rs` re-centres the hidden cursor itself on X11, Windows and macOS.

The world is stored in 16×16×256 columns, generated on server threads and streamed run-length encoded over TCP (length-prefixed bincode). The client meshes 16³ sections on worker threads with flood-filled sky and block light, smooth lighting and ambient occlusion.

## Tests

```sh
cargo test --workspace
```

Tests cover blocks, crafting recipes, world generation, villages, the realms, cities, turning OpenStreetMap data into streets and buildings (from a sample, not a live download), saving a real place, physics, trains on curves and slopes, cars climbing kerbs, redstone (a lever lighting lamps down a wire, a torch inverting, a button opening a door), portal frames lighting and breaking, the protocol, the server (join, chunks, edits, saving, dropped items coming back, riding a boat, crossing to the Sky realm), flowing water, meshing, input routing through Rune's headless runtime, chat focus, and two play sessions:

- **Survival**, driven through Rune's real mouse and keyboard input: walk to a tree, hold the mouse to punch the bottom log out, pick up the dropped log, press E, put the log in the 2×2 grid, take out planks, lay four planks for a crafting table, put it on the hotbar, press its number, right-click to place it, and right-click it to open the 3×3 grid.
- **Multiplayer**: two players join one server, see each other, walk, break a block and chat.

The [Build workflow](.github/workflows/build.yml) runs the tests and builds the game on macOS, Linux and Windows for every push.

The game can render itself without a window, which is how the screenshots above were made: `cargo run -- --snap shot.png --scene chicago` (scenes: `play`, `house` (builds a house by aiming and right-clicking like a player), `village`, `trade`, `train`, `cars`, `redstone`, `ember`, `sky`, `newyork`, `tokyo`, `paris`, `london`, `chicago`, `sydney`, `dubai`, `rome`, `night`, `rain`, `friends`, `inventory`, `creative`, `map`), or any camera with `RUNECRAFT_AT="x,y,z,yaw,pitch" cargo run -- --snap shot.png --scene at`.

## License

MIT
