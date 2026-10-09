# sinhonor

Dishonored's first-person movement and Blink in Rust, with a Bevy test course to try them in.

It isn't a rewrite of the game. It's a portable **motion kit**: walk, sprint, crouch, jump, fall, slide, mantle, lean, swim, ladder, Blink, the sword and the drop assassination, built the way [2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup) brings Skate 3 into MW2. The movement lives in its own crate, `crates/dis_motion`, which only depends on `glam`. It knows nothing about Bevy, so the same controller can be dropped into any game that can answer a box sweep.

**No game files ship with this repository.** Every number, sound, effect, texture, mesh and animation is read at startup **from your own installed copy** of Dishonored.

## Run it

You need Rust (https://rustup.rs) and Dishonored installed. From this folder:

```
cargo run --release -p sinhonor_demo
```

The first build takes a few minutes (Bevy is big); after that it's quick.

**Finding the install.** It uses `--game "<path to steamapps/common/Dishonored>"` if you give it, then the `DISHONORED_DIR` environment variable, then common Steam library paths, including the extra libraries listed in Steam's `libraryfolders.vdf`. `--difficulty easy|normal|hard|veryhard` picks which of Corvo's attribute sets is used (Normal by default).

On Linux the demo runs natively on Wayland with a real pointer lock. Under X11 the cursor is confined and re-centred instead.

## Controls

| Action | Keyboard / mouse |
|---|---|
| Move / look | WASD / mouse |
| Jump, or mantle when a ledge is in reach | Space |
| Crouch (toggle, like the game's default binding) | Ctrl or C |
| Slide | Crouch while sprinting |
| Sprint / slow walk | Shift / Alt |
| Lean left / right | Q / E |
| Blink: hold to aim, release to go | Right mouse or F |
| Blink tier I / II | 1 / 2 |
| Sword attack (or drop assassination while falling onto a guard) | Left mouse |
| Mute game sounds | M |
| Blink debug gizmos (target footprint and ground line) | G |
| Show or hide Corvo's arms and sword | H |
| Reset to spawn | R |
| Toggle help | F1 |
| Free the cursor (press again to quit) | Esc |

## The moves

Distances are in metres; the game works in Unreal units, where 1 uu = 1 cm.

- **Walk, run, sprint, sneak**: Corvo runs at 4 m/s, sprints at 6 m/s, sneaks crouched at 2.75 m/s and slow-walks at 2 m/s. These are the `m_GroundSpeed*` attributes from Corvo's release tweak (`Twk_Pawn_Corvo_Release`), per difficulty. Strafing and backing up are slower, with separate multipliers for running, sneaking and sprinting (`m_GroundStrafeMultiplier*`, `m_GroundBackwardMultiplier*`): sprinting sideways is 60% and backwards 55%. Acceleration is 20 m/s² (`m_AccelerationRate`), with UE3's ground friction of 8.
- **Jump**: launches at 8 m/s straight up (`m_JumpZ`, the same attribute the game's `StatePlayerMasterJump` reads) under 15 m/s² of gravity (`DefaultGame.ini`). That's a 2.1 m apex. The vertical speed of whatever you stand on is added, as the game does. Air control is 20% (`m_fAirControl`), and it never pushes you past the speed cap.
- **Fall**: touching down faster than 22.5 m/s reports fall damage (`m_MaxSpeedBeforeFallingDamage`; there's no health, so the HUD logs it). The landing dip and the camera shake scale with how hard you hit.
- **Crouch**: the collision shrinks from 1.75 m tall to 58 cm (`DishonoredPawn`'s cylinder and `CrouchHeight`), keeping your feet in place. Standing up only happens if there's room.
- **Auto-crouch**: walk at a gap only a crouched Corvo fits through and he crouches by himself, then stands once the way ahead is clear (`m_fAutoCrouchTestDistance` 1 m, `m_fAutoCrouchMinCrawlDepth`).
- **Slide**: crouch while sprinting. Your speed bleeds from where you started down to sneak speed over the game's 1.1 s (`m_fSlideTime`). The first half can't be cancelled (`m_fSlidePercentNotCancelable` 0.5); after that, crouch stands you up and jump jumps out of it.
- **Mantle**: jump at a ledge, or hold forward or jump while falling past one. The ledge finder walks up the wall in 12 cm steps, finds the top, checks there's room for you (standing, or else crouched), then climbs. The heights come from `m_pMantleTweaks`:
  - under 44 cm: you just step up (`MaxStepHeight`);
  - 55 cm–1.2 m: a low mantle, the game's quick step-up blend;
  - 1.2–1.7 m: medium;
  - 1.7–2.3 m: high;
  - over 2.3 m: too high, so Blink up.

  Each kind lasts as long as its animation (`Sword_Ready_MantleLow`, `…Medium`, `…High`, and the sneak variants). Falling faster than 10 m/s you can only catch edges at least 1.55 m high (`m_fFallSpeedForLedgeGrab`, `m_fLedgeGrabMantleMinEdgeHeight`), and edges, faces and tops have to be within the tweak's angle limits.
- **Lean**: Q and E lean up to 15°. The lean is a spring (`DefaultCamera.ini`'s springiness 80 and damping 12) that tilts the camera by half the angle, and it is clamped so your head never goes through a wall.
- **Swim**: walk into deep water and you swim at up to 6 m/s (`m_WaterSpeed`). Like the game, you move in strokes: strong acceleration for 0.3 s, then a weaker glide (`DefaultPlayerState.ini`'s swim block). Buoyancy holds your eyes at the surface. Swim at a ledge and jump to climb out.
- **Ladder**: walk into one to grab on. You climb at 2.54 m/s (`LadderSpeed`) toward where you look, and step off onto the landing at the top.
- **Camera**: head bob and roll scale with speed (the bob and roll amounts from `DefaultCamera.ini`), landings dip the view on a spring, and the FOV is the game's 75°.

### Blink

Blink reproduces the behaviour of the game's `DishonoredActivePowerComponent_Blink`, specified in our own words in `NOTES.md` §5. Its numbers come from `Twk_Blink`'s per-level array.

- **Range: a squashed sphere.** Looking level or down, you reach 11 m (tier I) or 16 m (tier II). Looking up, the aim is scaled until its height reaches 5 m, so you can blink a long way across but only 5 m up.
- **Targeting** runs every frame while you hold the button. A 20 cm box is swept along your aim. If it hits a wall close in front of you (within 3 m) the ray is nudged up, then sideways, 30 cm at a time, so you can blink past corners. From the hit it **pulls back** in half-radius steps until your whole body fits, but never behind the camera. That's why blinking at a wall puts you just in front of it. Then it drops a sweep to find the ground and raises the target so you fit above it.
- **The marker** is the game's own particle systems. `Blink_Ground_01` sits on the ground under the target, pitched 90° down as the game places it. `Blink_Fall_01` floats at the target when it's above the ground. `Blink_Mantle_01` shows when the target is a ledge. The vertical beam is the systems' far LOD, which only spawns beyond 2.56 m, as in the game.
- **Travel**: on release, physics switches to flying, and if you're standing Corvo crouches so low gaps fit (the target stays the same point, so a crouched body arrives with its feet higher, which is how Blink reaches ledges above you). He then moves 1 m every 10 ms (about 100 m/s), in sub-moves of at most 50 cm, **without collision**: the targeting sweeps already made the path safe. Travel stops at the target, if it would pass it, or on touching a pawn.
- **Arrival**: your velocity from before the blink is restored, so blink-jumping keeps momentum. Then comes the game's post-blink ledge check (a step-up or a full mantle, using `m_pMantleBlinkTweaks`) and an attempt to stand up.
- **Cooldown**: 1 s, with the game's cooldown lens effect (`Twk_Blink_Cooldown`, white streaks thrown back past the camera) and a decaying wobble.
- **Screen effect**: the game's warm-up wobble, travel distortion and blur curves (`m_fMoveBlurMaxStrength` and friends) drive a dedicated lens pass with radial blur, a dark tunnel vignette, animated peripheral distortion and subtle colour separation, alongside the FOV punch. The aim point stays sharp, and the pass fades with the game's cooldown curve. HDR bloom softens the targeting glow; the arms and HUD remain crisp.

### Sword

Left mouse swings Corvo's sword. It follows the game's behaviour, specified in `NOTES.md` §5c, with the numbers from his sword's tweak (`DisTweaks_MeleeAttackPlayer`).

- **Chains**: the first swing is a forehand. Press again within 1 s (`m_fMaxChainAttackTime`) and the next is a backhand, then forehand again. A press during a swing's chain window is kept and starts the next swing as soon as the current one allows. Each swing's timing (when the blade can hit, the chain window, when the next may start, when it ends) comes from its animation's notifies.
- **Reach**: 2.1 m (`m_fMaxContextRange`), longer when you're moving forward faster than 4 m/s: 3.15 m at a sprint (the player pawn's `m_fRaySpeedScale` settings). The blade is a 1.2 m wide box swept from the camera along your view during the swing's attack zone.
- **Damage**: 10 per blow (the sword's `m_MeleeDamage`). Guards have 25 health, the game's default for a character, so the third blow kills. When a blow will kill, Corvo plays the finishing swing (`Sword_Ready_Fatality_Generic_*`) instead.
- **Sneak attack**: crouched, Corvo swings his sneak attack (`Sword_Sneak_Attack_Small`).
- **Walls**: if the blade meets the world before a character, the swing recoils (`..._Big`, or `..._BigChain` mid-chain) and the camera shakes (`m_fCamShake_OnHitEnv`).
- The HUD shows the targeted guard's health. The swings play their own sounds.

### Drop assassination

Fall onto a guard and attack, and Corvo kills them from above. With Blink it's the classic stealth kill: blink up beside or above a guard, then drop. It follows the game's behaviour, specified in `NOTES.md` §5b. The numbers come from Corvo's sword tweak (`DisTweaks_DropAssassinate`).

- **Finding a target**: while you fall, Corvo's collision box is swept along where the fall will take him over the next 4 s (`m_fHitWindowInSeconds`), assuming at least 1 m/s downward (`m_fMinDropDownVel`). If that path hits nothing, it is swept straight down instead. A guard it hits counts if you're falling, not rising, if your feet are above its torso, and if nothing stands between you. The HUD shows the prompt.
- **In reach** (your feet within 3.5 m of its torso, `m_fMaxDropDistToTarget`): attacking kills straight away.
- **Further up**: attacking locks on. Corvo stops drifting, his fall speed doubles, steering and powers are disabled, and the kill starts as soon as he's in reach. If the guard stops being a valid target on the way down, the fall carries on as normal.
- **The side** is whichever of the guard's front, back, left or right you're on. Each side has its own kill. The guard's half of the game's paired animation puts Corvo 50 cm in front of it, 80 cm to its side or 60 cm behind it, facing it. Those positions are read from the City Watch's animations in a mission package.
- **The kill**: Corvo plays the side's `Sword_Ready_Assassination_Drop*_Master` animation and its sounds. The view follows the animation's camera bone down over the body and back up. You get control back when the animation releases you (about 1.6 to 1.8 s in). Guards don't need to be unaware.

## Corvo's arms, sounds and effects (from your copy of Dishonored)

At startup the demo loads **Corvo's first-person arms, his sword, their textures and animations, the motion sounds and the particle effects** from your install. Nothing from Dishonored is included in this project; each run reads these from the files below.

| File (under `DishonoredGame/CookedPCConsole`) | What it provides |
|---|---|
| `Startup.upk` | Corvo's tweak tree (speeds, jump, mantle, slide, camera), his sword's attack and drop-assassination tweaks and damage, the default character health (`Twk_Pawn_DefaultNPC`), the sword (`Wpn_PlySwords.Wpn_PlySword01`), all 385 first-person animations (`Ply_*`) and their effect notifies, the swimming and lens-drip effects |
| `Engine.upk` | The arms (`Ply_Player.Skm_Player`) and their textures, engine defaults (floor angle, friction) |
| `DishonoredGame.upk` | Pawn collision and eye height, `Twk_Blink`, the Blink markers and lens effect, footstep, slide, landing and splash effects, and the cobble, rock and plank textures the test course is dressed with |
| `Textures.tfc` and the other `.tfc` caches | Texture mips, read by byte range |
| `Bank_Footsteps.pck`, `Bank_Player.pck`, `Bank_Power_Player.pck`, `Bank_UI_Ingame_Water.pck` | Footsteps per surface and gait, slides, landings, mantles, crouch and stand, fall wind, sprint breath, swim strokes, and Blink's warm-up, cast and fizzle |
| `Bank_Weapon.pck`, `Bank_Impact.pck` | The sword and impact sounds the drop-assassination and sword animations post |
| `L_Streets1_P.upk` (or another mission package with the City Watch's animations) | Where each side's drop assassination puts Corvo (the victims' `anchor_jnt`) |
| `../Config/DefaultGame.ini`, `DefaultPlayerState.ini`, `DefaultCamera.ini` | Gravity, lean, swim, FOV, bob and roll |

The movement needs only the tuning. Everything else is optional and skipped with a warning if it's missing.

**How the arms work.**

- **Animations**: Dishonored cooks its animations in Sony's Edge format (built for the PS3's SPUs), which is why other UE3 viewers can't play them. `crates/edge_anim` decodes them.
- **Layers**: a base layer plays the sword hand and body (`Sword_Ready_Idle/Walk/Run/Sprint`, `Sword_Sneak_*`, `Sword_SlideLoop`, `Empty_Swim*`, the mantles, `Sword_Ready_JumpLandSmall`, the sword swings and the drop assassinations). A left-arm layer plays the power hand (`Powers_Idle/Walk/Sprint/Jump`, then `Powers_Cast_Blink_In`, `…_Loop` and `…_Out` around a blink). Changes crossfade over 0.18 s.
- **Camera bone**: the view is the skeleton's `camera_jnt` bone, so the arms sit exactly where the game puts them at its 75° FOV.
- **Sword**: held on the `RightHandWpn` socket. It's put away for the unarmed (`Empty_*`) swim animations.
- **Drawing**: the arms and sword are skinned on the CPU and drawn by a second camera on their own layer, so they never clip into walls.
- **Effects from the animations**: when Blink is cast, the gold glow and smoke on the back of the hand come from the animations themselves. `Powers_Cast_Blink_In` and `_Loop` trigger `Ps_Tattoo_Glow_02` on the hand's `Tattoo` socket at 0 s (so it re-fires on every 0.33 s loop while you aim), and `_Out` triggers `Ps_Tattoo_Glow_04`. The swim strokes put splashes on the fingers the same way. The demo reads every one of these notifies, so the effects keep the game's timing.
- **Glowing tattoo**: a mask derived at runtime from the installed arm textures follows the skin's UVs and uses the material's gold power-hand colour. Its brightness rises while targeting and fades after travel. Depth fading softens smoke and glow cards where they meet the hand.

**Sound.** Wwise sound packages are parsed and their Vorbis audio converted to Ogg in memory. Footsteps follow your gait and the surface under you (stone, wood, gravel, water, roof tiles, metal). Set `SINHONOR_LOG_SFX=1` to print each cue as it plays.

**Effects.** The game's Cascade particle systems are simulated and drawn as sprites and mesh particles, at the game's distance LODs. They include the Blink marker's original swirl meshes and arrival streaks, slide dust, landing dust, footstep puffs on gravel and water, water splash, swimming wake, and drips on the lens after you climb out of water. Square sprites retain their square shape, including axis-locked glow cards. Hard landings also shake the camera.

**What's still approximate:**

- **Not yet exact**: walking uses UE3's standard `CalcVelocity` model, not Dishonored's own "LocoNew" walking path. The mantle's ledge finder, the slide and the lean are modelled from the tweak values and animation lengths.
- **Camera**: the head bob is procedural. The game drives it from a camera animation (`Ply_Nav_LocoCamera_at`), which the Edge decoder can now read but nothing plays yet.
- **Effects**: SubUV flipbooks and the original materials' scrolling/distortion graphs aren't done. Mesh particles use cooked LOD 0 geometry, UVs, vertex colours and rotation curves. The particle materials, tattoo brightness and Blink lens shader approximate the look; they do not reconstruct the original shaders. The world camera uses HDR bloom.
- **Smoke**: the hand smoke is simulated relative to the camera, so it doesn't trail behind you as it does in the game.
- **Agility** (power jump, double jump) isn't in yet.
- **Sword**: no impact sounds, sparks or blood yet (they come from the game's contact system). With several guards in one swing the kit hits the nearest, where the game orders them by the swing's direction. Guards don't fight back, so there's no blocking, parrying or sword locks.
- **Drop assassination**: the guards are boxes that topple over. No guard model plays its half of the kill yet, and the kill animations' blood lens effect isn't shown.

`NOTES.md` §6 has a fidelity table for every part.

## The test course

A blockout course dressed with the game's textures (`crates/sinhonor_demo/src/level.rs`). You start facing along it.

1. **Mantle wall**, 8 m ahead: five blocks of rising height, 30 cm (step up), 90 cm (low), 1.5 m (medium), 2.2 m (high) and 3.2 m (too high, so Blink onto it).
2. **Stairs** on the right, 20 cm risers up to a 3 m balcony.
3. **Gravel patch** on the left and a **puddle** on the right, for footsteps and their dust and splashes.
4. **Pool** behind you to the left: 4 m deep, for swimming and climbing out.
5. **Slide lane**: a crawl beam 1 m off the ground that only fits you crouched or sliding.
6. **Lean pillars** to peek round.
7. **Ladder tower**, 6 m up, then **rooftops** with gaps of 8 m, 10 m and 14 m. Tier I clears the first, and the last needs tier II.
8. **The perch**: an 11 m pillar you can only reach by blinking upward from the rooftops.
9. **Four dummy guards** (red boxes; the dark band is their front) with 25 health. Blink stops at them, the sword hurts them, and you can drop-assassinate any of them:
   - two in the open: blink up beside one and fall onto it;
   - one with its back to the balcony: walk off the edge onto it;
   - one below the first rooftop: walk off the roof, lock on and dive.

   **R** stands them back up.

## Using the motion kit in another game

`dis_motion` only needs two things from a host game:

- A `World` implementation. Only one method is required:
  - `sweep(start, end, half)`: move a box along a path and report the first blocking hit (point, normal, and whether it started inside something). A zero `half` is a line trace.

  The others have defaults: `overlaps`, `water` (the water volume at a point), `ladder`, `blink_blocked` (the game's blink-blocking volumes) and `pawn` (a character's centre, floor, torso height, facing and health, for the sword and the drop assassination). Every move is built on these, so any collision a host has will do. `BoxWorld` (axis-aligned boxes) is included.
- An `Input` each frame: move axes, look delta, and jump, crouch, sprint, walk, lean, Blink and attack buttons.

Then:

```rust
let data = dis_data::load(&install, dis_data::Difficulty::Normal)?;
let mut motion = Motion::new(MotionTuning::from_game(&data), spawn, yaw);
// every frame
let events = motion.update(&world, &input, dt);
```

Read back `motion.camera.eye`, `motion.yaw`, `motion.pitch`, `motion.camera.roll` and `motion.camera.fov_deg` for the camera, `motion.state` and `events` for animation and sound, `motion.blink` (mode, target and `fx` screen parameters) for Blink's visuals, and `motion.takedown` (the prompt, the lock-on and the kill under way) plus `events.drop_assassination` to kill the target on your side, and `motion.melee` (the swing playing and the target) plus `events.melee` (swings, hits with their damage, wall strikes) for the sword. Coordinates are Unreal-style: Z up, centimetres. Convert at your boundary.

## Tests

```
cargo test --workspace
```

`crates/dis_motion/tests/motion.rs` drives the controller with scripted input. It checks:

- settling on the floor and running at run speed;
- the jump apex against ballistics;
- stepping up small ledges and mantling tall ones;
- the slide bleeding to crouch speed;
- auto-crouching under a low gap;
- Blink reaching a wall and keeping momentum, staying on the floor when cast crouched, and its squashed-sphere range;
- swimming at the surface and climbing a ladder;
- the sword: forehand and backhand chaining, hitting a guard in reach and finishing it with a killing blow, recoiling off a wall, missing out of reach, the reach growing with forward speed, and the sneak attack when crouched;
- the drop assassination: a kill in reach, the lock-on dive from higher up, the side chosen from the target's facing, the landing place, sweeping Corvo's box, and no target when something is in the way, when rising or on the ground.

One test runs on your install's real tuning when it can find it, and skips otherwise.

## Where the numbers come from

All the tuning is read at runtime by `crates/dis_data`, and `cargo run --release -p dis_data --example dump_tuning` prints everything it found.

- **Corvo's tweak tree** (`Startup.upk`): the root is `Twk_Pawn_Corvo.Twk_Pawn_Corvo_Release`. It points at one attribute set per difficulty (each `DisAttribute` holds four values, Easy to VeryHard), the mantle tweaks, a separate mantle tweak used after a blink, and the camera tweaks.
- **Pawn defaults** (`DishonoredGame.upk`, `Engine.upk`): collision size, crouch size, step height, eye height, ladder speed, walkable floor angle and friction, from the class default objects.
- **INI files**: gravity, lean springs, swim strokes, FOV, bob and roll.
- **Animation lengths** (`Startup.upk`): mantles, slides and landings last as long as the animations that play them.
- **Behaviour**: Blink's targeting, travel, end and screen effect, and the jump, follow the behaviour specified in `NOTES.md` §5; the drop assassination follows §5b and the sword §5c. No game code, decompiled or otherwise, is in this repository.

## Layout

```
crates/dis_motion/     the motion kit: engine-agnostic, depends only on glam
  src/controller.rs    the state machine: walk, crouch, jump, fall, slide, mantle, swim, ladder, Blink travel
  src/blink.rs         Blink targeting, range, pull-back, stepping, cooldown and screen parameters
  src/melee.rs         the sword: swing choice and chaining, reach, target, blade sweep, wall recoil
  src/takedown.rs      drop assassination: target search, lock-on dive, side and landing place
  src/camera.rs        eye height, bob, roll, landing dip, lean spring, FOV
  src/collide.rs       collide-and-slide on top of World::sweep
  src/boxworld.rs      a World made of axis-aligned boxes, water, ladders and pawns
  src/tuning.rs        MotionTuning, built from the game's values
  tests/motion.rs      one test per move, by scripted input
crates/dis_data/       finds the install and loads tuning, sounds, effects, arms, textures and sword and drop-assassination data from it
crates/upk/            UE3 package reader: LZO, names, imports, exports, tagged properties, textures, skeletal/static meshes
crates/edge_anim/      Sony Edge animation decoder
crates/wwise/          Wwise sound packages (AKPK, bank v65) to Ogg
crates/cascade/        UE3 Cascade particle systems: loading, materials and simulation
crates/sinhonor_demo/  the Bevy test course
  src/main.rs          app, input, camera, sounds, effect triggers, HUD, autopilot
  src/hands.rs         Corvo's arms and sword: animation layers, CPU skinning, viewmodel camera
  src/blink_post.rs     Blink radial blur and lens distortion render pass (embedded WGSL)
  src/fx.rs            particle rendering, in the world, on the lens and in the viewmodel
  src/level.rs         the test course, as data
NOTES.md               research notes: formats, the game's motion and Blink, credits
```

The `examples/` of `upk`, `cascade`, `edge_anim`, `wwise` and `dis_data` are the dev tools used to work all this out: package dumps, particle simulation, texture and animation probes, and a reference finder.

## Screenshot mode

`cargo run --release -p sinhonor_demo -- --autopilot shots` plays a scripted route with no mouse needed: a mantle, a blink, the ladder, the rooftop blink, a slide, a close-up of the Blink marker and the arrival lens effect. It prints the motion state at each checkpoint, saves a PNG of each into `shots/`, and quits. Add `--route fx` for the effects route instead: the Blink marker near and far, mid-travel, gravel, puddle, falling into the pool, swimming and climbing out. `--route drop` plays the three drop assassinations: off the balcony, diving from the rooftop, and blinking up beside a guard. `--route sword` chains three swings into a guard, swings at a wall and makes a sneak attack. It's handy for checking nothing broke after a change.

## Purpose and scope

sinhonor is an independent, non-commercial research and interoperability project. Its aim is to understand how Dishonored's player movement, Blink, sword and drop assassination feel, and to make that feel usable in other games through original code.

- **You need your own copy.** It works only with a legitimately purchased install. The demo reads that install on your machine, at runtime.
- **Nothing from the game is distributed.** This repository contains no game assets, data, configuration, executable code or decompiled code, and none is generated into it. The `.gitignore` is a whitelist so none can be committed by accident.
- **The game is left untouched.** Nothing here modifies, patches or injects into the installed game or its files. It doesn't run alongside the game, doesn't connect to any online service, and doesn't bypass any copy protection or access control.
- **The code is original.** The file-format readers are written from public format documentation (credited below and in `NOTES.md`) and from examining the file formats so the data can be read. The movement, Blink, sword and drop-assassination code implements a behaviour specification written in our own words. No code from the game or from other projects' decompilations is used.

Dishonored and its content belong to ZeniMax Media and Arkane Studios. If you hold rights in that content and have a concern about anything here, please open an issue and it will be addressed promptly.

## Credits and licenses

Built with help from UE Viewer, UELib, ue3-tools, dishonoredrecompiled's format notes, CodeRed-Generator, ww2ogg and lewton, on [Bevy](https://bevyengine.org) and [glam](https://github.com/bitshifter/glam-rs). Structural references: [iw4L](https://github.com/vladtrc/iw4L), [gang-beasts-rust](https://github.com/muffinmxn/gang-beasts-rust), [benilla](https://github.com/samwhosung/benilla) and [2010-rust-rewrite-mashup](https://github.com/chasmlol/2010-rust-rewrite-mashup); [dishonored-bevy](https://github.com/Eamo5/dishonored-bevy) was consulted for facts only. `NOTES.md` §8 has the full list with licenses.

This project is dual-licensed under MIT or Apache-2.0. Dishonored is a trademark of ZeniMax Media; this project is not affiliated with Arkane Studios or Bethesda.
