# sinhonor

A portable Rust **motion kit** built from Dishonored (2012): the player's movement
(walk, sprint, crouch, jump, fall, slide, mantle, lean, swim, ladder) and the motion powers
(Blink, Agility), packaged so they can be dropped into other games. It's modelled on how
[2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) brings Skate 3 into MW2.

**No game files ship with this repository.** All tuning (speeds, jump, mantle heights, Blink
range and stepping) is read **at runtime from your own installed copy** of Dishonored.

## Layout

| crate | what it does |
|---|---|
| `crates/upk` | Reader for Dishonored's UE3 packages: LZO chunk flattening, name/import/export tables, tagged properties, textures, skeletal meshes |
| `crates/edge_anim` | Decoder for the Sony Edge animation format Dishonored cooks its animations in |
| `crates/dis_data` | Finds your install and loads the motion and Blink tuning, sounds, effects and first-person arms from it |
| `crates/wwise` | Reader for the Wwise sound packages (AKPK, bank v65); resolves events and converts clips to Ogg |
| `crates/cascade` | Loads and simulates Unreal Engine 3 Cascade particle systems (with their materials and textures) from the install |
| `crates/dis_motion` | Engine-agnostic movement and Blink logic. The host game supplies collision through a trait |
| `crates/sinhonor_demo` | Bevy test level that shows the motion kit in action |

## Running

```
cargo run --release -p sinhonor_demo -- --game "/path/to/steamapps/common/Dishonored"
```

`--game` defaults to the `DISHONORED_DIR` environment variable, then to common Steam library paths
(including extra libraries listed in `libraryfolders.vdf`). `--difficulty easy|normal|hard|veryhard` selects
which of Corvo's attribute sets is used.

| Key | Action |
|---|---|
| WASD / mouse | move / look |
| Space | jump, or mantle when a ledge is in reach |
| Ctrl or C | crouch toggle; while sprinting: slide |
| Shift / Alt | sprint / slow walk |
| Q / E | lean |
| Right mouse or F | hold to aim Blink, release to go |
| 1 / 2 | Blink tier I / II |
| M | mute game sounds |
| G | Blink debug gizmos (target footprint and ground line) |
| H | show or hide Corvo's arms and sword |
| R / F1 / Esc | reset / help / free cursor (again to quit) |

Sound effects (footsteps per surface, landings, mantle, slide, Blink and more) are decoded at startup from
your install's Wwise packages. Set `SINHONOR_LOG_SFX=1` to print each cue as it plays.

Effects are the game's own particle systems, simulated and drawn as sprites: the Blink targeting marker
(ground and fall variants), the Blink arrival wind on the camera lens, slide dust, landing dust, footstep puffs
on gravel and water, water splash, swimming wake, and drips on the lens after leaving water. Hard landings
also shake the camera.

On Linux the demo runs natively on Wayland, using a real pointer lock. Under X11 the cursor is confined and re-centred instead.

`--autopilot <dir>` plays a scripted route (mantle, blink, ladder, rooftop blink, slide, close-up Blink
marker and arrival lens), prints the motion state at each checkpoint and writes screenshots to `<dir>`.
Add `--route fx` for the ground and water effects route (gravel, puddle, pool, swimming, climbing out).

## Using the motion kit in another game

Implement `dis_motion::World` (a box sweep plus optional water, ladder and blink-blocker queries) over your
physics, build `MotionTuning::from_game(&dis_data::load(..)?)`, and call `Motion::update(&world, &input, dt)`
each frame. Read back `motion.camera.eye`, `motion.yaw` and `motion.pitch`, `motion.camera.roll` and `fov_deg`,
plus `motion.blink.fx` for the lens effect. Coordinates are Unreal-style (Z up, cm). Convert at your boundary.

## Tests

```
cargo test --workspace
```

One test uses your install when it can find it, and skips otherwise.

## Research notes

`NOTES.md` records what was learned about the file formats and the game's motion and Blink
behaviour, written in our own words. Decompiled code and game data are never committed.

## Credits and licenses

See `NOTES.md` §Credits. This project is dual-licensed under MIT or Apache-2.0.
Dishonored is a trademark of ZeniMax Media; this project is not affiliated with Arkane Studios or Bethesda.
